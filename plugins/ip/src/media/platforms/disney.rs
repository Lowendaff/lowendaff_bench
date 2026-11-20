//! Disney+ unlock probe: checks storefront redirect headers and page location metadata.

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

/// Platform specification for Disney+.
pub const SPEC: Platform = Platform {
    id: "disney",
    name: "Disney+",
    probe,
};

/// Probes Disney+'s web root over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let web_req = HttpRequest::get(
            family,
            parse::endpoint(base_url, SPEC.id, "web", "https://www.disneyplus.com/"),
        );
        match parse::send(ctx, web_req).await {
            Ok(web) => parse(&web),
            Err(verdict) => verdict,
        }
    })
}

/// Checks Disney+'s initial web response for immediate geo-block redirects.
#[must_use]
pub fn web_early(web: &HttpResponse) -> Option<Verdict> {
    if let Some(early) = parse::gate(web) {
        return Some(early);
    }
    if let Some(loc) = web.header("location") {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("unavailable") || lower.contains("unsupported") {
            return Some(Verdict::no());
        }
        if lower.contains("preview") || lower.contains("coming-soon") {
            return Some(Verdict::No {
                restriction: Some(Restriction::ComingSoon),
            });
        }
    }
    if web.status == 403 || web.status == 451 {
        return Some(Verdict::no());
    }
    None
}

/// Evaluates Disney+'s web response.
#[must_use]
pub fn parse(web: &HttpResponse) -> Verdict {
    if let Some(early) = web_early(web) {
        return early;
    }
    let body = web.text();
    if body.contains("Disney+ is not available in your region")
        || body.contains("unavailable in your location")
    {
        return Verdict::no();
    }
    let region = web
        .header("location")
        .and_then(parse::url_country)
        .or_else(|| {
            parse::extract_between(&body, "\"countryCode\":\"", "\"").and_then(parse::country_code)
        });
    match region {
        Some(code) => Verdict::yes_in(code),
        None => parse::no_region(),
    }
}
