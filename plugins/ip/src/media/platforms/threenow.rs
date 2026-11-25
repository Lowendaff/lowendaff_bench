//! ThreeNow New Zealand Sky Free live-EPG + MediaTailor HLS availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

const FALLBACK_LSAI_HLS: &str = concat!(
    "https://sky-three-lsai-prod.fullscreen.nz/v1/session/",
    "db5eac5fd0a381d9ac535832f897ecd44a6f32c9/sky-three-prod-v1/index.m3u8"
);

/// Platform specification for ThreeNow.
pub const SPEC: Platform = Platform {
    id: "threenow",
    name: "ThreeNow",
    probe,
};

/// Probes ThreeNow over `family`: fetches the live EPG from `now-api.fullscreen.nz/v5/live-epg`
/// to discover the active Sky NZ MediaTailor `lsai.csab` HLS endpoint, then checks geo-access.
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
        let epg_req = HttpRequest::get(
            family,
            "https://now-api.fullscreen.nz/v5/live-epg".to_owned(),
        );
        let stream_url = ctx
            .probe(&epg_req)
            .await
            .ok()
            .and_then(|res| extract_lsai_url(&res))
            .unwrap_or_else(|| FALLBACK_LSAI_HLS.to_owned());
        let check_req = HttpRequest::get(family, stream_url);
        match parse::send(ctx, check_req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

fn extract_lsai_url(response: &HttpResponse) -> Option<String> {
    let val = parse::json(response)?;
    let channels = val.get("channels")?.as_array()?;
    for ch in channels {
        let candidate = ch
            .pointer("/videoRenditions/lsai/csab")
            .and_then(serde_json::Value::as_str)
            .map(str::trim);
        if let Some(url) = candidate
            && url.starts_with("https://")
            && url.contains(".fullscreen.nz/")
        {
            return Some(url.to_owned());
        }
    }
    None
}

/// Parses ThreeNow's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    parse::status_verdict(
        response,
        "NZ",
        &["geoblocked", "outside new zealand", "request blocked"],
    )
}
