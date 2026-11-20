//! Rakuten TV market probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Rakuten TV.
pub const SPEC: Platform = Platform {
    id: "rakuten-tv",
    name: "Rakuten TV",
    probe,
};

/// Probes Rakuten TV over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://gizmo.rakuten.tv/v3/me/start?classification_id=5&device_identifier=web"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Rakuten TV's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 404 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        if val.get("errors").is_some() {
            return Verdict::no();
        }
        let market = val
            .pointer("/data/market_code")
            .or_else(|| val.get("market_code"))
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code);
        if market.is_some() || (response.status == 200 && val.get("data").is_some()) {
            return Verdict::Yes { region: market };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
