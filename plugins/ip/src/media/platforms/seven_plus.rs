//! 7plus Australian Seven West Media market geolocation probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for 7plus.
pub const SPEC: Platform = Platform {
    id: "seven-plus",
    name: "7plus",
    probe,
};

/// Probes 7plus over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://market-cdn.swm.digital/v1/market/ip/?apikey=web"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses 7plus's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text().to_ascii_lowercase();
    if body.contains("australia only")
        || body.contains("403001")
        || body.contains("geoblock")
        || body.contains("geo_block")
        || body.contains("geo-block")
        || body.contains("outside australia")
        || body.contains("not available in your")
    {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let has_market = val.get("_id").is_some_and(|v| !v.is_null())
            || val
                .get("token")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|tok| !tok.trim().is_empty());
        if (200..300).contains(&response.status) && has_market {
            return Verdict::yes_in("AU");
        }
        if response.status == 200 {
            return parse::no_region();
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
