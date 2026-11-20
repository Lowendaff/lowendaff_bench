//! RaiPlay Italian `mediapolisvod` relinker probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for RaiPlay.
pub const SPEC: Platform = Platform {
    id: "rai-play",
    name: "RaiPlay",
    probe,
};

/// Probes RaiPlay over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://mediapolisvod.rai.it/relinker/relinkerServlet.htm?cont=2606803&output=64"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses RaiPlay's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451)
        || body.contains("video_no_available")
        || body.contains("geoprotetto")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("IT");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
