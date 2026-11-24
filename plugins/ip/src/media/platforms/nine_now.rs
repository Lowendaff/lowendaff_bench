//! 9Now Australian livestreams API probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for 9Now.
pub const SPEC: Platform = Platform {
    id: "nine-now",
    name: "9Now",
    probe,
};

/// Probes 9Now over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://tv-api.9now.com.au/v2/pages/livestreams?device=web"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses 9Now's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let geoblocked = val
            .get("geoblocked")
            .and_then(serde_json::Value::as_bool)
            .or_else(|| val.get("geoBlocked").and_then(serde_json::Value::as_bool))
            .or_else(|| {
                val.pointer("/data/geoblocked")
                    .and_then(serde_json::Value::as_bool)
            });
        match geoblocked {
            Some(true) => return Verdict::no(),
            Some(false) if (200..300).contains(&response.status) => return Verdict::yes_in("AU"),
            _ => {}
        }
        let error_geo = ["error", "code", "message"].into_iter().any(|key| {
            val.get(key)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|msg| {
                    let lower = msg.to_ascii_lowercase();
                    lower.contains("geoblock")
                        || lower.contains("geo_block")
                        || lower.contains("outside australia")
                })
        });
        if error_geo {
            return Verdict::no();
        }
    } else {
        let lower = response.text().to_ascii_lowercase();
        if lower.contains("geoblocked")
            || lower.contains("geo_blocked")
            || lower.contains("outside australia")
        {
            return Verdict::no();
        }
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
