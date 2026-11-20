//! CBC Gem Canadian `radio-canada` validation probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for CBC Gem.
pub const SPEC: Platform = Platform {
    id: "cbc-gem",
    name: "CBC Gem",
    probe,
};

/// Probes CBC Gem over `family`.
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
                "https://services.radio-canada.ca/media/validation/v2/",
                "?appCode=medianetlive&connectionType=hd&deviceType=ipad",
                "&idMedia=cbuft&multibitrate=true&output=json"
            )),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses CBC Gem's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let error_code = val.get("errorCode").and_then(serde_json::Value::as_i64);
        let message = val
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if error_code == Some(0) || val.get("url").and_then(serde_json::Value::as_str).is_some() {
            return Verdict::yes_in("CA");
        }
        if error_code.is_some()
            || message.contains("not available")
            || message.contains("outside canada")
        {
            return Verdict::no();
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
