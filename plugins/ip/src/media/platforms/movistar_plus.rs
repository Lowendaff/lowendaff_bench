//! Movistar Plus+ Spanish availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Movistar Plus+.
pub const SPEC: Platform = Platform {
    id: "movistar-plus",
    name: "Movistar Plus+",
    probe,
};

/// Probes Movistar Plus+ over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://contratar.movistarplus.es/"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Movistar Plus+'s HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text().to_lowercase();
    let geo_blocked = [
        "fuera de espana",
        "fuera de espa\u{00f1}a",
        "fuera de espa&ntilde;a",
        "fuera de espa&#241;a",
        "fuera de espa&#xf1;a",
        "no disponible en tu pais",
        "no disponible en tu pa\u{00ed}s",
        "no disponible en tu pa&iacute;s",
        "no disponible en tu pa&#237;s",
        "no disponible en tu pa&#xed;s",
    ]
    .iter()
    .any(|marker| body.contains(marker));
    if matches!(response.status, 403 | 451) || geo_blocked {
        return Verdict::no();
    }
    parse::no_region()
}
