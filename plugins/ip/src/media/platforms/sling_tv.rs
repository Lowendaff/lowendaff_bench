//! Sling TV US `p-geo.movetv.com` geolocation probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Sling TV.
pub const SPEC: Platform = Platform {
    id: "sling-tv",
    name: "Sling TV",
    probe,
};

/// Probes Sling TV over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://p-geo.movetv.com/geo"))
            .with_header("origin", "https://www.sling.com")
            .with_header("referer", "https://www.sling.com/");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Sling TV's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let disallowed = val
            .get("playback_disallowed")
            .and_then(serde_json::Value::as_bool);
        let allowed = val
            .get("playback_allowed")
            .and_then(serde_json::Value::as_bool);
        if disallowed == Some(true) || allowed == Some(false) {
            return Verdict::no();
        }
        if (200..300).contains(&response.status)
            && (allowed == Some(true) || disallowed == Some(false))
        {
            return Verdict::yes_in("US");
        }
    }
    parse::status_verdict(
        response,
        "US",
        &[
            "only available inside the us",
            "unavailable in your location",
            "outside of us",
        ],
    )
}
