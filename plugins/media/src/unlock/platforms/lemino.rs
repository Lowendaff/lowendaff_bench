//! Lemino Japan delivery ready probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Lemino.
pub const SPEC: Platform = Platform {
    id: "lemino",
    name: "Lemino",
    probe,
};

/// Probes Lemino over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::post(
            family,
            url("https://if.lemino.docomo.ne.jp/v1/user/delivery/watch/ready"),
            br#"{"CheckTarget":[{"kind":"1","cid":"0099999999","lid":"0099999999"}]}"#.to_vec(),
        )
        .with_header("content-type", "application/json");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Lemino's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 400 | 200) {
        return Verdict::yes_in("JP");
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
