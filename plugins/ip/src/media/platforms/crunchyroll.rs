//! Crunchyroll anonymous client token country probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Crunchyroll.
pub const SPEC: Platform = Platform {
    id: "crunchyroll",
    name: "Crunchyroll",
    probe,
};

/// Probes Crunchyroll over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::post(
            family,
            url("https://www.crunchyroll.com/auth/v1/token"),
            b"grant_type=client_id".to_vec(),
        )
        .with_header("content-type", "application/x-www-form-urlencoded")
        .with_header("authorization", "Basic bm9haWhkZXZtXzZpeWcwYThsMHE6");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Crunchyroll's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451) || body.contains("forbidden_by_country") {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        if val.get("error").is_some() {
            return Verdict::no();
        }
        let country = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code);
        if country.is_some() || val.get("access_token").is_some() {
            return Verdict::Yes { region: country };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
