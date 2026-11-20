//! Binge Australian Streamotion auth probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Binge.
pub const SPEC: Platform = Platform {
    id: "binge",
    name: "Binge",
    probe,
};

/// Probes Binge over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::post(
            family,
            url("https://auth.streamotion.com.au/oauth/token"),
            br#"{"client_id":"check","grant_type":"password"}"#.to_vec(),
        )
        .with_header("content-type", "application/json");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Binge's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    match response.status {
        200 | 400 | 401 => Verdict::yes_in("AU"),
        403 | 451 => Verdict::no(),
        status => Verdict::Unexpected(format!("HTTP {status}")),
    }
}
