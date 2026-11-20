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
    let body = response.text();
    if body.contains("\"geoblocked\":true") || body.contains("GeoBlocked") {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("AU");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
