//! Hulu US welcome availability probe.

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

/// Platform specification for Hulu.
pub const SPEC: Platform = Platform {
    id: "hulu",
    name: "Hulu",
    probe,
};

/// Probes Hulu over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://www.hulu.com/welcome"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Hulu's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if let Some(loc) = response.header("location") {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("hulu.jp") {
            return Verdict::yes_in("JP");
        }
        if lower.contains("unavailable") || lower.contains("geo") {
            return Verdict::no();
        }
        if lower.contains("disneyplus") {
            return Verdict::Unexpected("redirected to Disney+".to_owned());
        }
    }
    let body = response.text().to_ascii_lowercase();
    if body.contains("anonymous proxy tool") || body.contains("unblocker") {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if matches!(response.status, 403 | 451) || body.contains("not available in your region") {
        return Verdict::no();
    }
    if (200..400).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
