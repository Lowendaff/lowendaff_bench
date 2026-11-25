//! Apple TV+ unlock probe via Apple's geolocation country service.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Apple TV+.
pub const SPEC: Platform = Platform {
    id: "apple-tv",
    name: "Apple TV+",
    probe,
};

/// Probes Apple TV+ over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://gspe1-ssl.ls.apple.com/pep/gcc"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Apple TV+'s HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if body
        .to_ascii_lowercase()
        .contains("not available in your country")
        || response
            .final_url
            .to_ascii_lowercase()
            .contains("/unsupported")
    {
        return Verdict::no();
    }
    let region =
        parse::country_code(body.trim()).or_else(|| parse::url_country(&response.final_url));
    if let Some(code) = region.as_deref()
        && matches!(code, "CN" | "CU" | "IR" | "KP" | "RU" | "SY")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        match region {
            Some(code) => Verdict::yes_in(code),
            None => parse::no_region(),
        }
    } else {
        Verdict::Unexpected(format!("HTTP {}", response.status))
    }
}
