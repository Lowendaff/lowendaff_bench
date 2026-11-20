//! Coupang Play South Korea availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Coupang Play.
pub const SPEC: Platform = Platform {
    id: "coupang-play",
    name: "Coupang Play",
    probe,
};

/// Probes Coupang Play over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://www.coupangplay.com/"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Coupang Play's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location") {
        if loc.contains("not-available") || loc.contains("unavailable") {
            return Verdict::no();
        }
        return Verdict::yes_in("KR");
    }
    if response.status == 200 {
        return Verdict::yes_in("KR");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
