//! ESPN+ US BAMGrid session GraphQL probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

const ESPN_CLIENT_KEY: &str =
    "ZXNwbiZicm93c2VyJjEuMC4w.ptUt7QxsteaRruuPmGZFaJByOoqKvDP2a5YkInHrc7c";

/// Platform specification for ESPN+.
pub const SPEC: Platform = Platform {
    id: "espn-plus",
    name: "ESPN+",
    probe,
};

/// Probes ESPN+ via BAMGrid device registration and session GraphQL check over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let bearer = format!("Bearer {ESPN_CLIENT_KEY}");
        let dev_req = HttpRequest::post(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "devices",
                "https://espn.api.edge.bamgrid.com/devices",
            ),
            concat!(
                r#"{"deviceFamily":"browser","applicationRuntime":"chrome","#,
                r#""deviceProfile":"windows","attributes":{}}"#
            )
            .as_bytes()
            .to_vec(),
        )
        .with_header("authorization", &bearer)
        .with_header("content-type", "application/json");
        let dev_res = match parse::send(ctx, dev_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        let assertion = match extract_field(&dev_res, "assertion") {
            Ok(tok) => tok,
            Err(verdict) => return verdict,
        };

        let form = format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
             &latitude=0&longitude=0&platform=browser&subject_token={assertion}\
             &subject_token_type=urn%3Abamtech%3Aparams%3Aoauth%3Atoken-type%3Adevice"
        );
        let tok_req = HttpRequest::post(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "token",
                "https://espn.api.edge.bamgrid.com/token",
            ),
            form.into_bytes(),
        )
        .with_header("authorization", &bearer)
        .with_header("content-type", "application/x-www-form-urlencoded");
        let tok_res = match parse::send(ctx, tok_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        let refresh = match extract_field(&tok_res, "refresh_token") {
            Ok(tok) => tok,
            Err(verdict) => return verdict,
        };

        let gql_body = serde_json::json!({
            "query": concat!(
                "mutation refreshToken($input: RefreshTokenInput!) { ",
                "refreshToken(refreshToken: $input) { ",
                "activeSession { inSupportedLocation location { countryCode } } ",
                "} }"
            ),
            "variables": { "input": { "refreshToken": refresh } }
        })
        .to_string()
        .into_bytes();
        let gql_req = HttpRequest::post(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "check",
                "https://espn.api.edge.bamgrid.com/graph/v1/device/graphql",
            ),
            gql_body,
        )
        .with_header("authorization", ESPN_CLIENT_KEY)
        .with_header("content-type", "application/json");
        match parse::send(ctx, gql_req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

fn extract_field(response: &HttpResponse, key: &str) -> Result<String, Verdict> {
    if let Some(early) = parse::gate(response) {
        return Err(early);
    }
    let lower = response.text().to_ascii_lowercase();
    if matches!(response.status, 403 | 451)
        || lower.contains("forbidden-location")
        || lower.contains("403 error")
    {
        return Err(Verdict::no());
    }
    if !(200..300).contains(&response.status) {
        return Err(Verdict::Unexpected(format!("HTTP {}", response.status)));
    }
    let Some(val) = parse::json(response) else {
        return Err(Verdict::Unexpected(format!("HTTP {}", response.status)));
    };
    val.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Verdict::Unexpected(format!("missing ESPN+ {key}")))
}

/// Parses ESPN+'s HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location")
        && (loc.contains("unavailable") || loc.contains("block") || loc.contains("denied"))
    {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let session = val
            .pointer("/data/refreshToken/activeSession")
            .or_else(|| val.pointer("/data/registerDevice/session"))
            .or_else(|| val.pointer("/extensions/sdk/session"))
            .unwrap_or(&val);
        let supported = session
            .get("inSupportedLocation")
            .and_then(serde_json::Value::as_bool);
        let country = session
            .pointer("/location/countryCode")
            .or_else(|| session.get("countryCode"))
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code);
        match (supported, country.as_deref()) {
            (Some(false), _) => return Verdict::no(),
            (_, Some(cc)) if cc != "US" => return Verdict::no(),
            (Some(true), Some("US")) if (200..300).contains(&response.status) => {
                return Verdict::yes_in("US");
            }
            _ => {}
        }
    }
    let body = response.text().to_ascii_lowercase();
    if body.contains("not available in your region") || body.contains("forbidden-location") {
        return Verdict::no();
    }
    parse::no_region()
}
