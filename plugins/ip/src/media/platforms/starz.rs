//! Starz two-step `AuthTokenAuthorization` + `/api/v4/User/geolocation` US probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Restriction;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Starz.
pub const SPEC: Platform = Platform {
    id: "starz",
    name: "Starz",
    probe,
};

/// Probes Starz in two steps over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let tok_req = HttpRequest::get(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "token",
                "https://www.starz.com/sapi/header/v1/starz/us/109448574b2147ccbc494b429ff5ef1b",
            ),
        )
        .with_header("referer", "https://www.starz.com/us/en/");
        let tok_res = match parse::send(ctx, tok_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        if let Some(early) = parse::gate(&tok_res) {
            return early;
        }
        let auth_token = tok_res.text().trim().to_owned();
        if matches!(tok_res.status, 403 | 451)
            || auth_token.to_ascii_lowercase().contains("accessdenied")
        {
            return Verdict::no();
        }
        if tok_res.status != 200 || auth_token.is_empty() {
            return Verdict::Unexpected(format!("HTTP {}", tok_res.status));
        }

        let geo_req = HttpRequest::get(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "check",
                "https://auth.starz.com/api/v4/User/geolocation",
            ),
        )
        .with_header("authtokenauthorization", auth_token)
        .with_header("bestavailabletoken", "true")
        .with_header("origin", "https://www.starz.com")
        .with_header("referer", "https://www.starz.com/")
        .with_header("x-client-features", "DeviceCount");
        match parse::send(ctx, geo_req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Starz's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if let Some(val) = parse::json(response) {
        let props = val.get("properties");
        let bool_field = |key: &str| {
            val.get(key)
                .and_then(serde_json::Value::as_bool)
                .or_else(|| {
                    props
                        .and_then(|p| p.get(key))
                        .and_then(serde_json::Value::as_bool)
                })
        };
        let vpn = bool_field("isKnownProxy");
        if vpn == Some(true) {
            return Verdict::No {
                restriction: Some(Restriction::ProxyDetected),
            };
        }
        let allowed = bool_field("isAllowedCountry");
        let access = bool_field("isAllowedAccess");
        if allowed == Some(false) || access == Some(false) || matches!(response.status, 403 | 451) {
            return Verdict::no();
        }
        if !(200..300).contains(&response.status) {
            return Verdict::Unexpected(format!("HTTP {}", response.status));
        }
        let country = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code)
            .or_else(|| {
                props
                    .and_then(|p| p.get("country"))
                    .and_then(serde_json::Value::as_str)
                    .and_then(parse::country_code)
            });
        return match allowed {
            Some(true) => Verdict::Yes {
                region: Some(country.unwrap_or_else(|| "US".to_owned())),
            },
            Some(false) => Verdict::no(),
            None => Verdict::Unexpected("missing Starz isAllowedCountry".to_owned()),
        };
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
