//! Pluto TV regional storefront redirect probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Pluto TV.
pub const SPEC: Platform = Platform {
    id: "pluto-tv",
    name: "Pluto TV",
    probe,
};

/// Probes Pluto TV over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://pluto.tv/"))
            .with_redirect(RedirectPolicy::Follow { max: 5 });
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Pluto TV's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let final_lower = response.final_url.to_ascii_lowercase();
    if final_lower.contains("/intl")
        || final_lower.contains("plutotv-is-not-available")
        || final_lower.contains("thanks-for-watching")
    {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let has_error = match val.get("error") {
            None | Some(serde_json::Value::Null | serde_json::Value::Bool(false)) => false,
            Some(serde_json::Value::String(text)) => !text.trim().is_empty(),
            Some(serde_json::Value::Array(items)) => !items.is_empty(),
            Some(serde_json::Value::Object(map)) => !map.is_empty(),
            Some(_) => true,
        };
        if has_error {
            return Verdict::no();
        }
        let country = val
            .pointer("/session/countryCode")
            .or_else(|| val.get("countryCode"))
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code);
        let has_token = val
            .get("sessionToken")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|tok| !tok.trim().is_empty());
        if (200..300).contains(&response.status) && (country.is_some() || has_token) {
            return Verdict::Yes { region: country };
        }
    }
    let lower_body = response.text().to_ascii_lowercase();
    if lower_body.contains("plutotv-is-not-available")
        || lower_body.contains("thanks-for-watching")
        || lower_body.contains("not available in your")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        let region = parse::url_country(&response.final_url).or_else(|| Some("US".to_owned()));
        return Verdict::Yes { region };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
