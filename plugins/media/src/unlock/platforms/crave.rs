//! Crave Canadian `9c9media` package probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Crave.
pub const SPEC: Platform = Platform {
    id: "crave",
    name: "Crave",
    probe,
};

/// Probes Crave over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url(concat!(
                "https://capi.9c9media.com/destinations/crave_web",
                "/platforms/desktop/contents/2236485/contentPackages"
            )),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Crave's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451)
        || body.contains("GeoRestriction")
        || body.contains("geo")
    {
        return Verdict::no();
    }
    if matches!(response.status, 200 | 401) {
        return Verdict::yes_in("CA");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
