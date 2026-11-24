//! Twitch public GraphQL country probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Twitch.
pub const SPEC: Platform = Platform {
    id: "twitch",
    name: "Twitch",
    probe,
};

/// Probes Twitch over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::post(
            family,
            url("https://gql.twitch.tv/gql"),
            br#"{"query":"query{requestInfo{countryCode}}"}"#.to_vec(),
        )
        .with_header("client-id", "kimne78kx3ncx6brgo4mv6wki5h1ko")
        .with_header("content-type", "application/json");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Twitch's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let has_errors = val.get("errors").is_some_and(|err| {
            !err.is_null() && err.as_array().is_none_or(|items| !items.is_empty())
        });
        if (200..300).contains(&response.status) && !has_errors {
            let country = val
                .pointer("/data/requestInfo/countryCode")
                .and_then(serde_json::Value::as_str)
                .and_then(parse::country_code);
            if let Some(code) = country.as_deref() {
                return if matches!(code, "CN" | "KP" | "RU") {
                    Verdict::no()
                } else {
                    Verdict::yes_in(code)
                };
            }
            if val
                .get("data")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|map| !map.is_empty())
            {
                return parse::no_region();
            }
        }
        if has_errors {
            return Verdict::Unexpected("Twitch GraphQL error".to_owned());
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
