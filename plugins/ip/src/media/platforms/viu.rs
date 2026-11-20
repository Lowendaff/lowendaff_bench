//! Viu OTT regional redirect probe.

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

/// Platform specification for Viu.
pub const SPEC: Platform = Platform {
    id: "viu",
    name: "Viu",
    probe,
};

/// Probes Viu over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://www.viu.com/"))
            .with_redirect(RedirectPolicy::Follow { max: 5 });
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Viu's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let lower_url = response.final_url.to_ascii_lowercase();
    if lower_url.contains("no-service") || matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(seg) = parse::extract_between(&lower_url, "/ott/", "/")
        && let Some(code) = parse::country_code(seg)
    {
        return Verdict::yes_in(code);
    }
    let body = response.text();
    if body.contains("no-service") || body.contains("not available in your country") {
        return Verdict::no();
    }
    if response.status == 200 {
        let region =
            parse::extract_between(&body, "\"countryCode\":\"", "\"").and_then(parse::country_code);
        Verdict::Yes { region }
    } else {
        Verdict::Unexpected(format!("HTTP {}", response.status))
    }
}
