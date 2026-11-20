//! ESPN+ US availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for ESPN+.
pub const SPEC: Platform = Platform {
    id: "espn-plus",
    name: "ESPN+",
    probe,
};

/// Probes ESPN+ over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://plus.espn.com/"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
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
    let body = response.text().to_ascii_lowercase();
    if body.contains("not available in your region") {
        return Verdict::no();
    }
    parse::no_region()
}
