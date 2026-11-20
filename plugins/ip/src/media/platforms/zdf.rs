//! ZDF German `geo.txt` probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for ZDF.
pub const SPEC: Platform = Platform {
    id: "zdf",
    name: "ZDF",
    probe,
};

/// Probes ZDF over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://ssl.zdf.de/geo/de/geo.txt"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses ZDF's HTTP response into a [`Verdict`].
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
    let trimmed = body.trim();
    if trimmed.eq_ignore_ascii_case("XX") || trimmed.eq_ignore_ascii_case("NON-EU") {
        return Verdict::no();
    }
    match parse::country_code(trimmed) {
        Some(code) if code == "DE" => Verdict::yes_in("DE"),
        Some(_) => Verdict::no(),
        None => parse::no_region(),
    }
}
