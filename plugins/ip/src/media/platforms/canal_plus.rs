//! Canal+ French `boutique-tunnel` geo-redirect probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Canal+.
pub const SPEC: Platform = Platform {
    id: "canal-plus",
    name: "Canal+",
    probe,
};

/// Probes Canal+ over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://boutique-tunnel.canalplus.com/"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Canal+'s HTTP response into a [`Verdict`].
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
        if lower.contains("othercountry")
            || lower.contains("other-country")
            || lower.contains("unavailable")
            || lower.contains("geo")
        {
            return Verdict::no();
        }
        if (300..400).contains(&response.status) && lower.contains("canalplus.com") {
            return Verdict::yes_in("FR");
        }
    }
    let lower_body = response.text().to_ascii_lowercase();
    if lower_body.contains("othercountry") || lower_body.contains("other-country-blocking") {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("FR");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
