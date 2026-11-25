//! Molotov French `fapi` `is-france` probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Molotov.
pub const SPEC: Platform = Platform {
    id: "molotov",
    name: "Molotov",
    probe,
};

/// Probes Molotov over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://fapi.molotov.tv/v1/open-europe/is-france"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Molotov's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if (200..300).contains(&response.status)
        && let Some(val) = parse::json(response)
    {
        let is_fr = val
            .get("is_france")
            .and_then(serde_json::Value::as_bool)
            .or_else(|| {
                val.pointer("/geo/is_france")
                    .and_then(serde_json::Value::as_bool)
            });
        let country = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code)
            .or_else(|| {
                val.pointer("/geo/country")
                    .and_then(serde_json::Value::as_str)
                    .and_then(parse::country_code)
            });
        return match (is_fr, country.as_deref()) {
            (Some(true), _) | (None, Some("FR")) => Verdict::yes_in("FR"),
            (Some(false), _) | (None, Some(_)) => Verdict::no(),
            (None, None) => Verdict::Unexpected("unrecognized Molotov payload".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
