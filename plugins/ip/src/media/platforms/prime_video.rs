//! Amazon Prime Video unlock probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Restriction;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Prime Video.
pub const SPEC: Platform = Platform {
    id: "prime-video",
    name: "Prime Video",
    probe,
};

/// Probes Prime Video over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://www.primevideo.com/"))
            .with_redirect(RedirectPolicy::Follow { max: 5 });
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Prime Video's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if response.status != 200 {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    }
    let body = response.text();
    if body.contains("\"isServiceRestricted\":true") || body.contains("ServiceAreaRestriction") {
        return Verdict::no();
    }
    if body.contains("vpn") && body.contains("proxy") && body.contains("HTTPProxy") {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if let Some(region) =
        parse::extract_between(&body, "\"currentTerritory\":\"", "\"").and_then(parse::country_code)
    {
        return Verdict::Yes {
            region: Some(region),
        };
    }
    Verdict::Unexpected("missing currentTerritory".to_owned())
}
