//! LiTV Taiwan VOD `getUrl` probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for LiTV.
pub const SPEC: Platform = Platform {
    id: "litv",
    name: "LiTV",
    probe,
};

/// Probes LiTV over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::post(
            family,
            url("https://www.litv.tv/vod/ajax/getUrl"),
            concat!(
                r#"{"type":"noauth","assetId":"vod44868-000001M001_800K","#,
                r#""puid":"00000000-0000-0000-0000-000000000000"}"#
            )
            .as_bytes()
            .to_vec(),
        )
        .with_header("content-type", "application/json");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses LiTV's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if body.to_ascii_lowercase().contains("outsideregionerror") {
        return Verdict::no();
    }
    if response.status == 200
        && let Some(val) = parse::json(response)
    {
        let has_path = val
            .get("fullpath")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| !s.trim().is_empty());
        let no_err = val
            .get("errorMessage")
            .is_some_and(serde_json::Value::is_null);
        if has_path || no_err {
            return Verdict::yes_in("TW");
        }
        return Verdict::Unexpected("unrecognized LiTV payload".to_owned());
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
