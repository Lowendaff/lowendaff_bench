//! ITVX UK simulcast playlist probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for ITVX.
pub const SPEC: Platform = Platform {
    id: "itv-x",
    name: "ITVX",
    probe,
};

/// Probes ITVX over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://simulcast.itv.com/playlist/itvonline/ITV"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses ITVX's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    match response.status {
        200 | 401 => Verdict::yes_in("GB"),
        403 | 451 => Verdict::no(),
        status => Verdict::Unexpected(format!("HTTP {status}")),
    }
}
