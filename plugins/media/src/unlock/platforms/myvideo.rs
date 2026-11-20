//! MyVideo Taiwan `serviceAreaBlock` probe.

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

/// Platform specification for MyVideo.
pub const SPEC: Platform = Platform {
    id: "myvideo",
    name: "MyVideo",
    probe,
};

/// Probes MyVideo over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://www.myvideo.net.tw/"))
            .with_redirect(RedirectPolicy::Follow { max: 5 });
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses MyVideo's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if response.final_url.contains("serviceAreaBlock") || matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    parse::status_verdict(response, "TW", &["serviceAreaBlock"])
}
