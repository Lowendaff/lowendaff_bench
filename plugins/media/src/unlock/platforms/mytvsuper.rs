//! myTV SUPER Hong Kong session region probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for myTV SUPER.
pub const SPEC: Platform = Platform {
    id: "mytvsuper",
    name: "myTV SUPER",
    probe,
};

/// Probes myTV SUPER over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://www.mytvsuper.com/api/auth/getSession/self/"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses myTV SUPER's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let region = val.get("region").and_then(serde_json::Value::as_i64);
        return match region {
            Some(1) => Verdict::yes_in("HK"),
            Some(_) => Verdict::no(),
            None => Verdict::Unexpected("missing myTV SUPER region".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
