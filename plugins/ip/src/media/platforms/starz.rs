//! Starz `/api/v4/User/geolocation` US probe.

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

/// Probes Starz over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://auth.starz.com/api/v4/User/geolocation"),
        );
        match parse::send(ctx, req).await {
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
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let allowed = val
            .get("isAllowedCountry")
            .and_then(serde_json::Value::as_bool);
        let vpn = val.get("isKnownProxy").and_then(serde_json::Value::as_bool);
        if vpn == Some(true) {
            return Verdict::No {
                restriction: Some(Restriction::ProxyDetected),
            };
        }
        let country = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code);
        return match allowed {
            Some(true) => Verdict::Yes {
                region: Some(country.unwrap_or_else(|| "US".to_owned())),
            },
            Some(false) => Verdict::no(),
            None => Verdict::Unexpected("missing Starz isAllowedCountry".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
