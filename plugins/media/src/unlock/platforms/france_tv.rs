//! france.tv video validation probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for france.tv.
pub const SPEC: Platform = Platform {
    id: "france-tv",
    name: "france.tv",
    probe,
};

/// Probes france.tv over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://k7.ftven.fr/videos/195392964"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses france.tv's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451)
        || body.contains("geoblocage")
        || body.contains("\"code\":2003")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("FR");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
