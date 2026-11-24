//! radiko Japan `/area` prefecture probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for radiko.
pub const SPEC: Platform = Platform {
    id: "radiko",
    name: "radiko",
    probe,
};

/// Probes radiko over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://radiko.jp/area"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses radiko's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if response.status != 200 {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    }
    let body = response.text();
    for token in body.split(|ch: char| !ch.is_ascii_alphanumeric()) {
        if token == "OUT" {
            return Verdict::no();
        }
        let is_jp = token == "JP"
            || token.strip_prefix("JP").is_some_and(|num| {
                (1..=2).contains(&num.len()) && num.bytes().all(|b| b.is_ascii_digit())
            });
        if is_jp {
            return Verdict::yes_in("JP");
        }
    }
    Verdict::Unexpected("unrecognized radiko area response".to_owned())
}
