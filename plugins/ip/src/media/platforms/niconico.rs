//! Niconico Japan-restricted watch page probe.

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

/// Platform specification for Niconico.
pub const SPEC: Platform = Platform {
    id: "niconico",
    name: "Niconico",
    probe,
};

/// Probes Niconico over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://www.nicovideo.jp/watch/so23017073"))
            .with_redirect(RedirectPolicy::Follow { max: 5 });
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Niconico's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if body.contains("same_region")
        || body.contains("notAllowedCountry")
        || body.contains("cannot be played in your country")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("JP");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
