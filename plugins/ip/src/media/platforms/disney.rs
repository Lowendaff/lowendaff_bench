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
    let lower = body.to_ascii_lowercase();
    if lower.contains("not available in your region")
        || lower.contains("unavailable in your location")
        || lower.contains("\"insupportedlocation\":false")
        || lower.contains("\"insupportedlocation\": false")
    {
        return Verdict::no();
    }
    if !(200..400).contains(&web.status) {
        return Verdict::Unexpected(format!("HTTP {}", web.status));
    }
    let region = web
        .header("location")
        .and_then(location_country)
        .or_else(|| {
            parse::extract_between(&body, "\"countryCode\":\"", "\"").and_then(parse::country_code)
        });
    match region {
        Some(code) => Verdict::yes_in(code),
        None => parse::no_region(),
    }
}

fn location_country(url_or_path: &str) -> Option<String> {
    let clean = url_or_path
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let path = if let Some(scheme_end) = clean.find("://") {
        let after_host = &clean[scheme_end + 3..];
        let slash = after_host.find('/')?;
        &after_host[slash..]
    } else if let Some(after_slashes) = clean.strip_prefix("//") {
        let slash = after_slashes.find('/')?;
        &after_slashes[slash..]
    } else {
        clean
    };
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        let parts: Vec<&str> = segment.split('-').collect();
        let candidate = match parts.as_slice() {
            [cc] if cc.len() == 2 => Some(*cc),
            [lang, region]
                if (2..=3).contains(&lang.len())
                    && lang.bytes().all(|b| b.is_ascii_alphabetic()) =>
            {
                Some(*region)
            }
            [lang, script, region]
                if (2..=3).contains(&lang.len())
                    && script.len() == 4
                    && lang.bytes().all(|b| b.is_ascii_alphabetic())
                    && script.bytes().all(|b| b.is_ascii_alphabetic()) =>
            {
                Some(*region)
            }
            _ => None,
        };
        if let Some(code) = candidate.and_then(parse::country_code) {
            return Some(code);
        }
    }
    None
}
