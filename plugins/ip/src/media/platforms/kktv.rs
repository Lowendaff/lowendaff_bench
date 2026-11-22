//! KKTV Taiwan `/v3/ipcheck` probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for KKTV.
pub const SPEC: Platform = Platform {
    id: "kktv",
    name: "KKTV",
    probe,
};

/// Probes KKTV over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://api.kktv.me/v3/ipcheck"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses KKTV's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let data = val.get("data").unwrap_or(&val);
        let allowed = data.get("is_allowed").and_then(serde_json::Value::as_bool);
        let country = data
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code);
        return match (allowed, country.as_deref()) {
            (Some(true), Some(code)) => Verdict::yes_in(code),
            (Some(true), None) | (None, Some("TW")) => Verdict::yes_in("TW"),
            (Some(false), _) | (_, Some(_)) => Verdict::no(),
            (None, None) => Verdict::Unexpected("unrecognized KKTV payload".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
