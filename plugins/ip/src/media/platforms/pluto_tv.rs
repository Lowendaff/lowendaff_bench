//! Pluto TV `boot.pluto.tv` start probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

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
        let req = HttpRequest::get(
            family,
            url(concat!(
                "https://boot.pluto.tv/v4/start?appName=web&appVersion=7.0.0",
                "&deviceVersion=131.0.0&deviceModel=web&deviceMake=chrome&deviceType=web",
                "&clientID=00000000-0000-4000-8000-000000000000&clientModelNumber=1.0.0"
            )),
        );
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
    if let Some(val) = parse::json(response) {
        if val.get("error").is_some() {
            return Verdict::no();
        }
        let country = val
            .pointer("/session/countryCode")
            .or_else(|| val.get("countryCode"))
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code);
        if country.is_some() || val.get("sessionToken").is_some() {
            return Verdict::Yes { region: country };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
