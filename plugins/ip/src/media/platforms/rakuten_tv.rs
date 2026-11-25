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
        let req = HttpRequest::post(
            family,
            url("https://gizmo.rakuten.tv/v3/me/start?classification_id=5&device_identifier=web"),
            concat!(
                r#"{"affiliate_code":"","av1_support":false,"#,
                r#""device_serial":"00000000-0000-4000-8000-000000000000","#,
                r#""player":"web:PD-NONE"}"#
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

/// Parses Rakuten TV's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let has_errors = match val.get("errors") {
            None | Some(serde_json::Value::Null | serde_json::Value::Bool(false)) => false,
            Some(serde_json::Value::Array(items)) => !items.is_empty(),
            Some(serde_json::Value::Object(map)) => !map.is_empty(),
            Some(serde_json::Value::String(text)) => !text.trim().is_empty(),
            Some(_) => true,
        };
        if has_errors {
            let lower = response.text().to_ascii_lowercase();
            let geo_err = response.status == 200
                || lower.contains("forbidden")
                || lower.contains("market")
                || lower.contains("country")
                || lower.contains("geo")
                || lower.contains("not_available")
                || lower.contains("unavailable");
            return if geo_err {
                Verdict::no()
            } else {
                Verdict::Unexpected(format!("HTTP {}", response.status))
            };
        }
        let market = [
            "/data/market_code",
            "/data/iso3166_code",
            "/market_code",
            "/iso3166_code",
        ]
        .into_iter()
        .find_map(|ptr| {
            val.pointer(ptr)
                .and_then(serde_json::Value::as_str)
                .and_then(parse::country_code)
        });
        let has_data = val
            .get("data")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|map| !map.is_empty());
        if (200..300).contains(&response.status) && (market.is_some() || has_data) {
            return Verdict::Yes { region: market };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
