//! Wavve South Korea streaming probe.

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

/// Platform specification for Wavve.
pub const SPEC: Platform = Platform {
    id: "wavve",
    name: "Wavve",
    probe,
};

/// Probes Wavve over `family`.
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
                "https://apis.wavve.com/fz/streaming?device=pc&partner=pooq",
                "&apikey=E5F3E0D30947AA5440556471321BB6D9&credential=none&service=wavve",
                "&pooqzone=none&region=kor&drm=wm&targetage=all",
                "&contentid=MV_C901_SG0000113303&contenttype=movie"
            )),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Wavve's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text();
    let lower = body.to_ascii_lowercase();
    if lower.contains("vpn") || lower.contains("proxy") {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if let Some(val) = parse::json(response) {
        let warn = val
            .get("warning_kr")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let code = val.get("resultcode").and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        });
        let geo_json = !warn.is_empty()
            || code == Some(551)
            || contains_wavve_marker(&val, "\u{bd88}\u{bc95}\u{c801}\u{c73c}\u{b85c}")
            || contains_wavve_marker(&val, "\u{d574}\u{c678}")
            || contains_wavve_ascii(&val, "overseas");
        if geo_json || matches!(response.status, 403 | 451) {
            return Verdict::no();
        }
        if (200..300).contains(&response.status) && code == Some(200) {
            return Verdict::yes_in("KR");
        }
        if (200..300).contains(&response.status) {
            return parse::no_region();
        }
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    }
    if matches!(response.status, 403 | 451)
        || body.contains("\"resultcode\":\"551\"")
        || body.contains("\u{bd88}\u{bc95}\u{c801}\u{c73c}\u{b85c}")
        || body.contains("\u{d574}\u{c678}")
        || lower.contains("overseas")
    {
        return Verdict::no();
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

fn contains_wavve_marker(value: &serde_json::Value, needle: &str) -> bool {
    match value {
        serde_json::Value::String(text) => text.contains(needle),
        serde_json::Value::Array(items) => items.iter().any(|v| contains_wavve_marker(v, needle)),
        serde_json::Value::Object(map) => map.values().any(|v| contains_wavve_marker(v, needle)),
        _ => false,
    }
}

fn contains_wavve_ascii(value: &serde_json::Value, needle: &str) -> bool {
    match value {
        serde_json::Value::String(text) => text.to_ascii_lowercase().contains(needle),
        serde_json::Value::Array(items) => items.iter().any(|v| contains_wavve_ascii(v, needle)),
        serde_json::Value::Object(map) => map.values().any(|v| contains_wavve_ascii(v, needle)),
        _ => false,
    }
}
