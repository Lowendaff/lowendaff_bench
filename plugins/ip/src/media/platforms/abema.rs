//! Abema Japan `/v1/ip/check` probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Abema.
pub const SPEC: Platform = Platform {
    id: "abema",
    name: "Abema",
    probe,
};

/// Probes Abema over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://api.abema.io/v1/ip/check?device=android"),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Abema's HTTP response into a [`Verdict`].
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
        let country = val
            .get("isoCountryCode")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code);
        return match country.as_deref() {
            Some("JP") => Verdict::yes_in("JP"),
            Some(_) => Verdict::no(),
            None => Verdict::Unexpected("missing Abema isoCountryCode".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
