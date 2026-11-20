//! YouTube Premium unlock probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Restriction;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for YouTube Premium.
pub const SPEC: Platform = Platform {
    id: "youtube",
    name: "YouTube Premium",
    probe,
};

/// Probes YouTube Premium over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://www.youtube.com/premium"))
            .with_header("cookie", "CONSENT=YES+cb.20210328-17-p0.en+FX+700")
            .with_redirect(RedirectPolicy::Follow { max: 5 });
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses YouTube Premium's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if response.final_url.contains("google.cn") {
        return Verdict::No {
            restriction: Some(Restriction::ChinaMainland),
        };
    }
    if response.status != 200 {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    }
    let body = response.text();
    if body.contains("YouTube replicates") && body.contains("google.cn") {
        return Verdict::No {
            restriction: Some(Restriction::ChinaMainland),
        };
    }
    if body.contains("YouTube Premium is not available in your country")
        || body.contains("not available in your country")
    {
        return Verdict::no();
    }
    let region = parse::extract_between(&body, "\"INNERTUBE_CONTEXT_GL\":\"", "\"")
        .or_else(|| parse::extract_between(&body, "\"countryCode\":\"", "\""))
        .and_then(parse::country_code);
    if region.is_some()
        || body.contains("YouTube and YouTube Music ad-free")
        || body.contains("Manage your membership")
        || body.contains("ytInitialData")
    {
        Verdict::Yes { region }
    } else {
        Verdict::Unexpected("unrecognized YouTube page".to_owned())
    }
}
