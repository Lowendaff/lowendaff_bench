//! Joyn German two-step anonymous auth + `entitlement-token` probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

const JOYN_API_KEY: &str = "36lp1t4wto5uu2i2nk57ywy9on1ns5yg";

/// Platform specification for Joyn.
pub const SPEC: Platform = Platform {
    id: "joyn",
    name: "Joyn",
    probe,
};

/// Probes Joyn over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        if let Some(base) = base_url {
            let req = HttpRequest::get(family, format!("{base}/{}/check", SPEC.id));
            return match parse::send(ctx, req).await {
                Ok(res) => parse(&res),
                Err(verdict) => verdict,
            };
        }
        let auth_req = HttpRequest::post(
            family,
            "https://auth.joyn.de/auth/anonymous".to_owned(),
            concat!(
                r#"{"client_id":"b74b9f27-a994-4c45-b7eb-5b81b1c856e7","#,
                r#""client_name":"web","#,
                r#""anon_device_id":"b74b9f27-a994-4c45-b7eb-5b81b1c856e7"}"#
            )
            .as_bytes()
            .to_vec(),
        )
        .with_header("content-type", "application/json");
        let auth_res = match parse::send(ctx, auth_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        if let Some(early) = parse::gate(&auth_res) {
            return early;
        }
        let Some(token) = parse::json(&auth_res).and_then(|val| {
            val.get("access_token")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        }) else {
            return Verdict::Unexpected(format!("HTTP {}", auth_res.status));
        };

        let ent_req = HttpRequest::post(
            family,
            "https://api.joyn.de/content/entitlement-token".to_owned(),
            br#"{"content_id":"daserste-de-hd","content_type":"LIVE"}"#.to_vec(),
        )
        .with_header("authorization", format!("Bearer {token}"))
        .with_header("x-api-key", JOYN_API_KEY)
        .with_header("content-type", "application/json")
        .with_header("origin", "https://www.joyn.de")
        .with_header("referer", "https://www.joyn.de/");
        match parse::send(ctx, ent_req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Joyn's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    let lower = body.to_ascii_lowercase();
    if lower.contains("ent_assetnotavailableincountry")
        || lower.contains("not available in your country")
        || lower.contains("geoblocked")
    {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response)
        && val
            .get("entitlement_token")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|tok| !tok.trim().is_empty())
    {
        return Verdict::yes_in("DE");
    }
    if response.status == 200 {
        return Verdict::yes_in("DE");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
