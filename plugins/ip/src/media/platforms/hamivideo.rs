//! Hami Video Taiwan OTT live play probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Hami Video.
pub const SPEC: Platform = Platform {
    id: "hamivideo",
    name: "Hami Video",
    probe,
};

/// Probes Hami Video over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://hamivideo.hinet.net/api/play.do?id=OTT_LIVE_0000001844&freeProduct=1"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Hami Video's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let code = val.get("code").and_then(serde_json::Value::as_str);
        let has_url = val
            .get("url")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|url| !url.trim().is_empty());
        let ok_status = (200..300).contains(&response.status);
        return match code {
            Some("06001-106") => Verdict::no(),
            Some("06001-107" | "06001-000" | "00000") if ok_status => Verdict::yes_in("TW"),
            Some(_) if ok_status && has_url => Verdict::yes_in("TW"),
            Some(_) if ok_status => Verdict::no(),
            None if ok_status => Verdict::Unexpected("missing Hami Video code".to_owned()),
            _ => Verdict::Unexpected(format!("HTTP {}", response.status)),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
