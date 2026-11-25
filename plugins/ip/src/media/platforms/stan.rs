//! Stan Australian session geo-block probe.

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

/// Platform specification for Stan.
pub const SPEC: Platform = Platform {
    id: "stan",
    name: "Stan",
    probe,
};

/// Probes Stan over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::post(
            family,
            url("https://api.stan.com.au/login/v1/sessions/web/account"),
            b"{}".to_vec(),
        )
        .with_header("content-type", "application/json");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Stan's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let lower = response.text().to_ascii_lowercase();
    if lower.contains("vpndetected")
        || (lower.contains("vpn") && (lower.contains("proxy") || lower.contains("unblocker")))
    {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if lower.contains("geoblocked")
        || lower.contains("accessdenied")
        || lower.contains("access denied")
        || lower.contains("outside australia")
        || matches!(response.status, 403 | 404 | 451)
    {
        return Verdict::no();
    }
    // Inside Australia, empty credentials pass the geo-gate and return 400 validation error.
    if matches!(response.status, 200 | 400) {
        return Verdict::yes_in("AU");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
