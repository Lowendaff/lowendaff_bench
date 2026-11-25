//! Viaplay Nordic and European country redirect probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Viaplay.
pub const SPEC: Platform = Platform {
    id: "viaplay",
    name: "Viaplay",
    probe,
};

/// Probes Viaplay over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://viaplay.com/"))
            .with_redirect(RedirectPolicy::Follow { max: 5 });
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Viaplay's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let lower_body = response.text().to_ascii_lowercase();
    if lower_body.contains("en-unavailability")
        || lower_body.contains("region-blocked")
        || lower_body.contains("not available in your")
    {
        return Verdict::no();
    }
    if (300..=399).contains(&response.status)
        && let Some(loc) = response.header("location")
    {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("region-blocked") || lower.contains("unavailable") {
            return Verdict::no();
        }
        if let Some(code) = parse::url_country(loc) {
            return Verdict::yes_in(code);
        }
    }
    if response.status == 200 {
        return match parse::url_country(&response.final_url) {
            Some(code) => Verdict::yes_in(code),
            None => parse::no_region(),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
