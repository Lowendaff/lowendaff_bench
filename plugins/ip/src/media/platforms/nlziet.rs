//! NLZIET Dutch availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for NLZIET.
pub const SPEC: Platform = Platform {
    id: "nlziet",
    name: "NLZIET",
    probe,
};

/// Probes NLZIET over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://app.nlziet.nl/"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses NLZIET's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location") {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("geo") || lower.contains("unavailable") {
            return Verdict::no();
        }
    }
    let body = response.text().to_ascii_lowercase();
    if body.contains("geoblock") || body.contains("not available in your country") {
        return Verdict::no();
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
