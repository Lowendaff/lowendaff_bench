//! TVNZ+ New Zealand live play API probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for TVNZ+.
pub const SPEC: Platform = Platform {
    id: "tvnz-plus",
    name: "TVNZ+",
    probe,
};

/// Probes TVNZ+ over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://apis-public-prod.tech.tvnz.co.nz/api/v1/web/play/page/categories/live"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses TVNZ+'s HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    parse::status_verdict(response, "NZ", &["geoblocked", "outside new zealand"])
}
