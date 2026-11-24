//! TVING South Korea stream info probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for TVING.
pub const SPEC: Platform = Platform {
    id: "tving",
    name: "TVING",
    probe,
};

/// Probes TVING over `family`.
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
                "https://api.tving.com/v2a/media/stream/info",
                "?apiKey=1e7952d0917d6aab1f0293a063697610&mediaCode=RV60891248"
            )),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses TVING's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let is_geo_msg = |msg: &str| {
        let lower = msg.to_ascii_lowercase();
        msg.contains("\u{d574}\u{c678}")
            || lower.contains("not service")
            || lower.contains("only available in south korea")
    };
    if let Some(val) = parse::json(response) {
        let header_blocked = matches!(
            val.pointer("/header/status")
                .and_then(serde_json::Value::as_i64),
            Some(403 | 451)
        );
        let code_blocked = [
            val.pointer("/body/result/code"),
            val.pointer("/header/code"),
        ]
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .any(|code| code == "024");
        let msg_blocked = [
            val.pointer("/header/message"),
            val.pointer("/body/result/message"),
        ]
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .any(is_geo_msg);
        if header_blocked || code_blocked || msg_blocked {
            return Verdict::no();
        }
        let populated = match val.pointer("/body/content").or_else(|| val.get("content")) {
            Some(serde_json::Value::Object(map)) => !map.is_empty(),
            Some(serde_json::Value::Array(items)) => !items.is_empty(),
            Some(serde_json::Value::Null) | None => false,
            Some(_) => true,
        };
        if (200..300).contains(&response.status) && populated {
            return Verdict::yes_in("KR");
        }
    }
    let body = response.text();
    if body.contains("\"code\":\"024\"") || is_geo_msg(&body) {
        return Verdict::no();
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
