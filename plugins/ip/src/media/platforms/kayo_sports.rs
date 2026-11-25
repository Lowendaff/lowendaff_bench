//! Kayo Sports Australian Streamotion billing offers probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Kayo Sports.
pub const SPEC: Platform = Platform {
    id: "kayo-sports",
    name: "Kayo Sports",
    probe,
};

/// Probes Kayo Sports over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://billingapi.streamotion.com.au/v2/offers/kayo/"),
        )
        .with_header("origin", "https://kayosports.com.au")
        .with_header("referer", "https://kayosports.com.au/");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Kayo Sports's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    parse::status_verdict(
        response,
        "AU",
        &[
            "geoblocked",
            "access denied",
            "not available in your region",
        ],
    )
}
