//! Spotify registration availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

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
        let req = HttpRequest::post(
            family,
            url("https://spclient.wg.spotify.com/signup/public/v1/account"),
            concat!(
                "birth_day=1&birth_month=1&birth_year=1995&collect_personal_info=undefined",
                "&creation_flow=&creation_point=https%3A%2F%2Fwww.spotify.com%2F",
                "&displayname=bench&gender=neutral&iagree=1",
                "&key=a1e486e2729f46d6bb368d6b2bcda326&platform=www",
                "&referrer=&send-email=0&thirdpartyemail=0"
            )
            .as_bytes()
            .to_vec(),
        )
        .with_header("content-type", "application/x-www-form-urlencoded");
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
    if let Some(val) = parse::json(response) {
        let status = val.get("status").and_then(serde_json::Value::as_i64);
        let launched = val
            .get("is_country_launched")
            .and_then(serde_json::Value::as_bool);
        let errors_proxy = val.get("errors").is_some_and(contains_proxy);
        if status == Some(320) || errors_proxy {
            return Verdict::No {
                restriction: Some(Restriction::ProxyDetected),
            };
        }
        if launched == Some(false)
            || matches!(status, Some(311 | 120))
            || matches!(response.status, 403 | 451)
        {
            return Verdict::no();
        }
        if !(200..300).contains(&response.status) {
            return Verdict::Unexpected(format!("HTTP {}", response.status));
        }
        let country = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code);
        let status_ok = status == Some(1)
            || (matches!(status, None | Some(20 | 110 | 130 | 140)) && country.is_some());
        return if launched == Some(true) || (launched.is_none() && status_ok) {
            Verdict::Yes { region: country }
        } else {
            Verdict::Unexpected("unrecognized Spotify response".to_owned())
        };
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

fn contains_proxy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => text.to_ascii_lowercase().contains("proxy"),
        serde_json::Value::Array(items) => items.iter().any(contains_proxy),
        serde_json::Value::Object(map) => map.values().any(contains_proxy),
        _ => false,
    }
}
