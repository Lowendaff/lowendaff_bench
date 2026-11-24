//! Niconico Japan-restricted watch page probe.

use std::time;
use std::time::SystemTime;

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

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
        let track = SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let real = format!(
            "https://www.nicovideo.jp/api/watch/v3_guest/so23017073\
             ?actionTrackId=AAAAAAAAAA_{track}"
        );
        let req = HttpRequest::get(family, url(&real))
            .with_header("x-frontend-id", "6")
            .with_header("x-frontend-version", "0");
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
    if let Some(val) = parse::json(response) {
        let status = val
            .pointer("/meta/status")
            .and_then(serde_json::Value::as_i64);
        let error = val
            .pointer("/meta/errorCode")
            .and_then(serde_json::Value::as_str);
        let reason = val
            .pointer("/data/reasonCode")
            .and_then(serde_json::Value::as_str);
        if reason == Some("DOMESTIC_VIDEO") || (error == Some("FORBIDDEN") && reason.is_none()) {
            return Verdict::no();
        }
        if (200..300).contains(&response.status) && status == Some(200) && reason.is_none() {
            return Verdict::yes_in("JP");
        }
        return Verdict::Unexpected(format!(
            "watch API status {}",
            status.map_or_else(|| "unknown".to_owned(), |code| code.to_string())
        ));
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
