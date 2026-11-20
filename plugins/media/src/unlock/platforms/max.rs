//! Max `/geo-availability` redirect and country header probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Max.
pub const SPEC: Platform = Platform {
    id: "max",
    name: "Max",
    probe,
};

/// Probes Max over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://www.max.com/"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Max's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location") {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("geo-availability") || lower.contains("unavailable") {
            return Verdict::no();
        }
        if let Some(code) = parse::url_country(loc) {
            return Verdict::yes_in(code);
        }
    }
    let region = response
        .header("x-wbd-country")
        .and_then(parse::country_code)
        .or_else(|| {
            parse::extract_between(&response.text(), "\"countryCode\":\"", "\"")
                .and_then(parse::country_code)
        });
    if matches!(response.status, 200 | 301 | 302 | 307 | 308) {
        return Verdict::Yes { region };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
