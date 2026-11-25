//! Sky Go UK/IE OAuth sign-in availability probe.

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

/// Platform specification for Sky Go.
pub const SPEC: Platform = Platform {
    id: "sky-go",
    name: "Sky Go",
    probe,
};

/// Probes Sky Go over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url(concat!(
                "https://skyid.sky.com/authorise/skygo",
                "?response_type=token&client_id=sky&appearance=compact&redirect_uri=skygo://auth"
            )),
        )
        .with_redirect(RedirectPolicy::Follow { max: 5 });
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Sky Go's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text().to_ascii_lowercase();
    if matches!(response.status, 403 | 451)
        || body.contains("access denied")
        || body.contains("don't have permission to access")
        || body.contains("geoblocked")
    {
        return Verdict::no();
    }
    if response.status == 200 && (body.contains("skygosignin") || body.contains("sign in</h3>")) {
        return Verdict::yes_in("GB");
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
