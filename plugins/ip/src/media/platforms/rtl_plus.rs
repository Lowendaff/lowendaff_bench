//! RTL+ DACH streaming availability probe via Bedrock `front-auth` JWT geolocation.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

const OIDC_URL: &str = "https://auth.rtl.de/auth/realms/rtlplus/protocol/openid-connect/token";
const TOKEN_URL: &str =
    "https://front-auth.rtlde.bedrock.tech/v2/rtlde/platforms/m6group_web/token";
const OIDC_BODY: &[u8] = concat!(
    "client_id=anonymous-user",
    "&client_secret=4bfeb73f-1c4a-4e9f-a7fa-96aa1ad3d94c",
    "&grant_type=client_credentials"
)
.as_bytes();

/// Platform specification for RTL+.
pub const SPEC: Platform = Platform {
    id: "rtl-plus",
    name: "RTL+",
    probe,
};

/// Probes RTL+ over `family` by exchanging an anonymous OIDC token for a Bedrock `front-auth` JWT.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let oidc_url = parse::endpoint(base_url, SPEC.id, "oidc", OIDC_URL);
        let oidc_req = HttpRequest::post(family, oidc_url, OIDC_BODY.to_vec())
            .with_header("content-type", "application/x-www-form-urlencoded");
        let oidc_res = match parse::send(ctx, oidc_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        if let Some(early) = parse::gate(&oidc_res) {
            return early;
        }
        let Some(access_token) = parse::json(&oidc_res)
            .and_then(|v| {
                v.get("access_token")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .or_else(|| base_url.map(|_| "mock-token".to_owned()))
        else {
            return Verdict::Unexpected(format!("OIDC HTTP {}", oidc_res.status));
        };

        let check_url = parse::endpoint(base_url, SPEC.id, "check", TOKEN_URL);
        let req = HttpRequest::get(family, check_url)
            .with_header("authorization", format!("Bearer {access_token}"))
            .with_header("x-customer-name", "rtlde")
            .with_header("x-client-release", "6.55.7")
            .with_header("x-auth-device-id", "2d694782-18cc-46ab-b6b2-c33b1f6faf6a")
            .with_header("origin", "https://plus.rtl.de")
            .with_header("referer", "https://plus.rtl.de/");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses RTL+'s Bedrock JWT response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if response.final_url.contains("geoblocking") || matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if response.status == 200
        && let Some(val) = parse::json(response)
    {
        let jwt_cc = val
            .get("token")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::jwt_payload)
            .and_then(|claims| {
                claims
                    .pointer("/geo/country_code")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            });
        let direct_cc = val
            .pointer("/geo/country_code")
            .or_else(|| val.get("country_code"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        if let Some(raw) = jwt_cc.or(direct_cc) {
            let cc = raw.trim().to_ascii_uppercase();
            if matches!(cc.as_str(), "DE" | "AT" | "CH" | "LI" | "LU") {
                return Verdict::yes_in(cc);
            }
            if !cc.is_empty() {
                return Verdict::no();
            }
        }
    }
    let body = response.text().to_ascii_lowercase();
    if body.contains("geoblocking") || body.contains("not available in your country") {
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
