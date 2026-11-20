//! TVING South Korea stream info probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for TVING.
pub const SPEC: Platform = Platform {
    id: "tving",
    name: "TVING",
    probe,
};

/// Probes TVING over `family`.
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
                "https://api.tving.com/v2a/media/stream/info",
                "?apiKey=1e7952d0917d6aab1f0293a063697610&mediaCode=RV60891248"
            )),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses TVING's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    // 024 or "\u{d574}\u{c678}" ("overseas" in Korean) marks geo-restriction.
    if body.contains("\"code\":\"024\"")
        || body.contains("\u{d574}\u{c678}")
        || body.contains("not service")
    {
        return Verdict::no();
    }
    if response.status == 200 && body.contains("header") {
        return Verdict::yes_in("KR");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
