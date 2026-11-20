//! Wavve South Korea streaming probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Restriction;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Wavve.
pub const SPEC: Platform = Platform {
    id: "wavve",
    name: "Wavve",
    probe,
};

/// Probes Wavve over `family`.
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
                "https://apis.wavve.com/fz/streaming?device=pc&partner=pooq",
                "&apikey=E5F3E0D30947AA5440556471321BB6D9&credential=none&service=wavve",
                "&pooqzone=none&region=kor&drm=wm&targetage=all",
                "&contentid=MV_C901_SG0000113303&contenttype=movie"
            )),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Wavve's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451)
        || body.contains("\"resultcode\":\"551\"")
        || body.contains("\u{d574}\u{c678}")
        || body.contains("overseas")
    {
        return Verdict::no();
    }
    if body.contains("warning_kr") || body.contains("\u{bd88}\u{bc95}\u{c801}\u{c73c}\u{b85c}") {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if body.contains("\"resultcode\":\"200\"") || body.contains("\"resultcode\":200") {
        return Verdict::yes_in("KR");
    }
    parse::no_region()
}
