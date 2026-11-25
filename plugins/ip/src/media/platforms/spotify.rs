//! Spotify regional market probe via `open.spotify.com` `appServerConfig`.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Restriction;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Spotify.
pub const SPEC: Platform = Platform {
    id: "spotify",
    name: "Spotify",
    probe,
};

/// Probes Spotify over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://open.spotify.com/"))
            .with_redirect(RedirectPolicy::Follow { max: 5 })
            .with_max_bytes(512 * 1024);
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Spotify's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if let Some(market) = extract_server_config_market(&body) {
        return market_verdict(&market);
    }
    if let Some(val) = parse::json(response) {
        if val.get("errors").is_some_and(contains_proxy) {
            return Verdict::No {
                restriction: Some(Restriction::ProxyDetected),
            };
        }
        let launched = val
            .get("is_country_launched")
            .and_then(serde_json::Value::as_bool);
        if launched == Some(false) {
            return Verdict::no();
        }
        if (200..300).contains(&response.status) {
            let code = val
                .get("market")
                .or_else(|| val.get("country"))
                .and_then(serde_json::Value::as_str)
                .and_then(parse::country_code);
            if let Some(code) = code {
                return market_verdict(&code);
            }
        }
    }
    if response.status != 200 {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    }
    Verdict::Unexpected("missing Spotify market".to_owned())
}

fn extract_server_config_market(html: &str) -> Option<String> {
    let (_, after_id) = html.split_once("id=\"appServerConfig\"")?;
    let (_, after_gt) = after_id.split_once('>')?;
    let (encoded, _) = after_gt.split_once("</script>")?;
    let decoded = parse::base64_decode(encoded.trim())?;
    let val: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    val.get("market")
        .or_else(|| val.get("countryCode"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse::country_code)
}

fn market_verdict(code: &str) -> Verdict {
    if matches!(code, "CN" | "CU" | "IR" | "KP" | "RU" | "SY") {
        Verdict::no()
    } else {
        Verdict::yes_in(code)
    }
}

fn contains_proxy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => text.to_ascii_lowercase().contains("proxy"),
        serde_json::Value::Array(items) => items.iter().any(contains_proxy),
        serde_json::Value::Object(map) => map.values().any(contains_proxy),
        _ => false,
    }
}
