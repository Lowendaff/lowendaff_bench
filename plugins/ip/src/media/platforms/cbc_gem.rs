//! CBC Gem Canadian `cbc-stats-top.js` geolocation probe.

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
            url("https://www.cbc.ca/g/stats/js/cbc-stats-top.js"),
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
        let message = val
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if message.contains("not available")
            || message.contains("outside canada")
            || message.contains("pas disponible")
            || message.contains("dans votre pays")
        {
            return Verdict::no();
        }
        if let Some(code) = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code)
        {
            return if code == "CA" && (200..300).contains(&response.status) {
                Verdict::yes_in("CA")
            } else {
                Verdict::no()
            };
        }
    }
    let body = response.text();
    if let Some(cc) = parse::extract_between(&body, "\"country\":\"", "\"")
        .or_else(|| parse::extract_between(&body, "\"country\": \"", "\""))
        .and_then(parse::country_code)
    {
        return if cc == "CA" && (200..300).contains(&response.status) {
            Verdict::yes_in("CA")
        } else {
            Verdict::no()
        };
    }
    parse::no_region()
}
