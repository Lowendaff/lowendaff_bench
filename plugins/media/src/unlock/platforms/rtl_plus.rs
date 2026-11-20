//! RTL+ German streaming availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for RTL+.
pub const SPEC: Platform = Platform {
    id: "rtl-plus",
    name: "RTL+",
    probe,
};

/// Probes RTL+ over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://plus.rtl.de/"))
            .with_redirect(RedirectPolicy::Follow { max: 5 });
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses RTL+'s HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if response.final_url.contains("geoblocking") || matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    parse::status_verdict(
        response,
        "DE",
        &["geoblocking", "not available in your country"],
    )
}
