//! NHK+ domestic Japan IP probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for NHK+.
pub const SPEC: Platform = Platform {
    id: "nhk-plus",
    name: "NHK+",
    probe,
};

/// Probes NHK+ over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://plus.nhk.jp/api/v1/ipcheck"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses NHK+'s HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location")
        && loc.to_ascii_lowercase().contains("web.nhk")
    {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let allowed = val
            .get("is_domestic")
            .and_then(serde_json::Value::as_bool)
            .or_else(|| {
                val.pointer("/data/is_domestic")
                    .and_then(serde_json::Value::as_bool)
            })
            .or_else(|| val.get("allowed").and_then(serde_json::Value::as_bool));
        let country = val
            .get("country_code")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code)
            .or_else(|| {
                val.pointer("/data/country_code")
                    .and_then(serde_json::Value::as_str)
                    .and_then(parse::country_code)
            });
        if allowed == Some(false) {
            return Verdict::no();
        }
        if (200..300).contains(&response.status) {
            return match (allowed, country.as_deref()) {
                (Some(true), _) | (None, Some("JP")) => Verdict::yes_in("JP"),
                (None, Some(_)) => Verdict::no(),
                (None, None) | (Some(false), _) => {
                    Verdict::Unexpected("unrecognized NHK+ payload".to_owned())
                }
            };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
