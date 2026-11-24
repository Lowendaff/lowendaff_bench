//! U-NEXT Japan player URL geo-code probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for U-NEXT.
pub const SPEC: Platform = Platform {
    id: "u-next",
    name: "U-NEXT",
    probe,
};

/// Probes U-NEXT over `family`.
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
                "https://video-api.unext.jp/api/1/player/url",
                "?title_code=SID0012977&episode_code=ED00067432&play_mode=caption"
            )),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses U-NEXT's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let as_code = |v: &serde_json::Value| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        };
        let status = val
            .pointer("/data/result_status")
            .and_then(as_code)
            .or_else(|| val.get("code").and_then(as_code));
        let ok_http = (200..300).contains(&response.status);
        return match status {
            // 467 is U-NEXT's overseas IP restriction code; 470/475/200/0 mean domestic auth check.
            Some(467) => Verdict::no(),
            Some(0 | 200 | 470 | 475) if ok_http => Verdict::yes_in("JP"),
            Some(code) => Verdict::Unexpected(format!("unrecognized U-NEXT code {code}")),
            None if ok_http => parse::no_region(),
            None => Verdict::Unexpected(format!("HTTP {}", response.status)),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
