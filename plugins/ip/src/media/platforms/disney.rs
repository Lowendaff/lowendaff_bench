//! Disney+ unlock probe via BAMGrid device token exchange and session GraphQL check.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Restriction;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

const BAMGRID_CLIENT_KEY: &str =
    "ZGlzbmV5JmJyb3dzZXImMS4wLjA.Cu56AgSfBTDag5NiRA81oLHkDZfu5L3CKadnefEAY84";

/// Platform specification for Disney+.
pub const SPEC: Platform = Platform {
    id: "disney",
    name: "Disney+",
    probe,
};

/// Probes Disney+ via BAMGrid device registration and storefront redirect check over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let bearer = format!("Bearer {BAMGRID_CLIENT_KEY}");
        let dev_req = HttpRequest::post(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "devices",
                "https://disney.api.edge.bamgrid.com/devices",
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
        let assertion = match extract_token_field(&dev_res, "assertion") {
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
                "https://disney.api.edge.bamgrid.com/token",
            ),
            form.into_bytes(),
        )
        .with_header("authorization", &bearer)
        .with_header("content-type", "application/x-www-form-urlencoded");
        let tok_res = match parse::send(ctx, tok_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        let refresh = match extract_token_field(&tok_res, "refresh_token") {
            Ok(tok) => tok,
            Err(verdict) => return verdict,
        };

        let gql_body = serde_json::json!({
            "query": concat!(
                "mutation refreshToken($input: RefreshTokenInput!) { ",
                "refreshToken(refreshToken: $input) { ",
                "activeSession { sessionId inSupportedLocation location { countryCode } } ",
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
                "graphql",
                "https://disney.api.edge.bamgrid.com/graph/v1/device/graphql",
            ),
            gql_body,
        )
        .with_header("authorization", BAMGRID_CLIENT_KEY)
        .with_header("content-type", "application/json");
        let web_req = HttpRequest::get(
            family,
            parse::endpoint(base_url, SPEC.id, "web", "https://www.disneyplus.com/"),
        )
        .with_redirect(RedirectPolicy::Follow { max: 5 });

        let (gql_out, web_out) = tokio::join!(ctx.probe(&gql_req), ctx.probe(&web_req));
        if let Ok(web_res) = &web_out
            && let Some(early) = web_early(web_res)
        {
            return early;
        }
        match gql_out {
            Ok(gql_res) => parse(&gql_res),
            Err(err) => parse::error_verdict(&err),
        }
    })
}

fn extract_token_field(response: &HttpResponse, key: &str) -> Result<String, Verdict> {
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
        .ok_or_else(|| Verdict::Unexpected(format!("missing Disney+ {key}")))
}

/// Checks Disney+'s web response for immediate geo-block or preview redirects.
#[must_use]
pub fn web_early(web: &HttpResponse) -> Option<Verdict> {
    if let Some(early) = parse::gate(web) {
        return Some(early);
    }
    let candidates = [web.header("location"), Some(web.final_url.as_str())]
        .into_iter()
        .flatten();
    for candidate in candidates {
        let lower = candidate.to_ascii_lowercase();
        if lower.contains("unavailable") || lower.contains("unsupported") {
            return Some(Verdict::no());
        }
        if lower.contains("preview") || lower.contains("coming-soon") {
            return Some(Verdict::No {
                restriction: Some(Restriction::ComingSoon),
            });
        }
    }
    if web.status == 403 || web.status == 451 {
        return Some(Verdict::no());
    }
    None
}

/// Evaluates Disney+'s GraphQL session or web response.
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = web_early(response) {
        return early;
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
        match (supported, country) {
            (Some(false), Some(_)) => {
                return Verdict::no();
            }
            (Some(false), None) => return Verdict::no(),
            (Some(true), Some(code)) if (200..300).contains(&response.status) => {
                return Verdict::yes_in(code);
            }
            (None, Some(code)) if (200..300).contains(&response.status) => {
                return Verdict::yes_in(code);
            }
            _ => {}
        }
    }
    let body = response.text();
    let lower = body.to_ascii_lowercase();
    if lower.contains("not available in your region")
        || lower.contains("unavailable in your location")
        || lower.contains("\"insupportedlocation\":false")
        || lower.contains("\"insupportedlocation\": false")
    {
        return Verdict::no();
    }
    if !(200..400).contains(&response.status) {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    }
    let region = response
        .header("location")
        .and_then(location_country)
        .or_else(|| {
            parse::extract_between(&body, "\"countryCode\":\"", "\"").and_then(parse::country_code)
        });
    match region {
        Some(code) => Verdict::yes_in(code),
        None => parse::no_region(),
    }
}

fn location_country(url_or_path: &str) -> Option<String> {
    let clean = url_or_path
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let path = if let Some(scheme_end) = clean.find("://") {
        let after_host = &clean[scheme_end + 3..];
        let slash = after_host.find('/')?;
        &after_host[slash..]
    } else if let Some(after_slashes) = clean.strip_prefix("//") {
        let slash = after_slashes.find('/')?;
        &after_slashes[slash..]
    } else {
        clean
    };
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        let parts: Vec<&str> = segment.split('-').collect();
        let candidate = match parts.as_slice() {
            [cc] if cc.len() == 2 => Some(*cc),
            [lang, region]
                if (2..=3).contains(&lang.len())
                    && lang.bytes().all(|b| b.is_ascii_alphabetic()) =>
            {
                Some(*region)
            }
            [lang, script, region]
                if (2..=3).contains(&lang.len())
                    && script.len() == 4
                    && lang.bytes().all(|b| b.is_ascii_alphabetic())
                    && script.bytes().all(|b| b.is_ascii_alphabetic()) =>
            {
                Some(*region)
            }
            _ => None,
        };
        if let Some(code) = candidate.and_then(parse::country_code) {
            return Some(code);
        }
    }
    None
}
