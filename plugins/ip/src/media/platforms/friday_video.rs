//! friDay Video Taiwan streaming probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for friDay Video.
pub const SPEC: Platform = Platform {
    id: "friday",
    name: "friDay Video",
    probe,
};

/// Probes friDay Video over `family`.
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
                "https://video.friday.tw/api2/streaming/get",
                "?streamingId=132287&streamingType=2&contentType=4&contentId=1&clientId="
            )),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses friDay Video's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let has_geo_marker = |text: &str| {
        text.contains("\u{6d77}\u{5916}")
            || text.contains("\u{5730}\u{533a}")
            || text.contains("\u{5730}\u{5340}")
    };
    if let Some(val) = parse::json(response) {
        let code = val.get("code").and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        });
        let message = val
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim();
        match code {
            Some(0) if (200..300).contains(&response.status) => return Verdict::yes_in("TW"),
            Some(1003) => return Verdict::no(),
            _ if has_geo_marker(message) => return Verdict::no(),
            Some(code) if message.is_empty() => {
                return Verdict::Unexpected(format!("streaming code {code}"));
            }
            Some(code) => {
                return Verdict::Unexpected(format!("streaming code {code}: {message}"));
            }
            None => {}
        }
    }
    let body = response.text();
    if body.contains("\"code\":1003") || has_geo_marker(&body) {
        return Verdict::no();
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
