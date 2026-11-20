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
        let status = val
            .pointer("/data/result_status")
            .or_else(|| val.get("code"))
            .and_then(serde_json::Value::as_i64);
        return match status {
            // 467 is U-NEXT's overseas IP restriction code; 470/475/200/0 mean domestic auth check.
            Some(467) => Verdict::no(),
            Some(0 | 200 | 470 | 475) => Verdict::yes_in("JP"),
            _ if response.status == 200 => Verdict::yes_in("JP"),
            _ => Verdict::Unexpected("unrecognized U-NEXT code".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
