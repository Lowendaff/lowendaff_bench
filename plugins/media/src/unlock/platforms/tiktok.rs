//! TikTok regional availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for TikTok.
pub const SPEC: Platform = Platform {
    id: "tiktok",
    name: "TikTok",
    probe,
};

/// Probes TikTok over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://www.tiktok.com/cdn-cgi/trace"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses TikTok's HTTP response into a [`Verdict`].
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
    let region = parse::trace_loc(&body).or_else(|| {
        parse::extract_between(&body, "\"region\":\"", "\"").and_then(parse::country_code)
    });
    match region.as_deref() {
        Some("CN" | "HK" | "IN" | "IR" | "KP" | "AF") => Verdict::no(),
        Some(code) => Verdict::yes_in(code),
        None => Verdict::Unexpected("missing TikTok region".to_owned()),
    }
}
