//! BBC iPlayer UK `mediaselector` `bbc_one_london` probe.

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

/// Platform specification for BBC iPlayer.
pub const SPEC: Platform = Platform {
    id: "bbc-iplayer",
    name: "BBC iPlayer",
    probe,
};

/// Probes BBC iPlayer over `family`.
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
                "https://open.live.bbc.co.uk/mediaselector/6/select/version/2.0",
                "/mediaset/pc/vpid/bbc_one_london/format/json"
            )),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses BBC iPlayer's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if let Some(val) = parse::json(response) {
        if let Some(result) = val.get("result").and_then(serde_json::Value::as_str) {
            if result.eq_ignore_ascii_case("geolocation") {
                return Verdict::no();
            }
            if result.eq_ignore_ascii_case("vpn") {
                return Verdict::No {
                    restriction: Some(Restriction::ProxyDetected),
                };
            }
        }
        if response.status == 200 && val.get("media").is_some() {
            return Verdict::yes_in("GB");
        }
    }
    let lower = response.text().to_ascii_lowercase();
    if lower.contains("geolocation") {
        return Verdict::no();
    }
    if lower.contains("vpn") {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
