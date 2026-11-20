//! DMM TV Japan foreign IP check probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for DMM TV.
pub const SPEC: Platform = Platform {
    id: "dmm",
    name: "DMM TV",
    probe,
};

/// Probes DMM TV over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://api.dmm.com/service/digitalapi/-/ip_check/"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses DMM TV's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451)
        || body.contains("\"is_foreign\":true")
        || body.contains("foreign service")
        || body.contains("not available in your region")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("JP");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
