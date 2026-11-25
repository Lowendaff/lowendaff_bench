//! Crave Canadian `9c9media` playback manifest geo-constraint probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

const DEFAULT_MANIFEST: &str = concat!(
    "https://capi.9c9media.com/destinations/crave_atexace/platforms/desktop",
    "/playback/contents/1240619/contentPackages/7410805/manifest.mpd"
);

/// Platform specification for Crave.
pub const SPEC: Platform = Platform {
    id: "crave",
    name: "Crave",
    probe,
};

/// Probes Crave over `family`, dynamically refreshing the `9c9media` content package if the pinned
/// manifest ever rotates (`404`).
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let req = HttpRequest::get(
            family,
            parse::endpoint(base_url, SPEC.id, "check", DEFAULT_MANIFEST),
        );
        let res = match parse::send(ctx, req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        if res.status != 404 || base_url.is_some() {
            return parse(&res);
        }
        let Some((cid, pid)) = discover_package(ctx, family).await else {
            return parse(&res);
        };
        let dyn_url = format!(
            "https://capi.9c9media.com/destinations/crave_atexace/platforms/desktop\
             /playback/contents/{cid}/contentPackages/{pid}/manifest.mpd"
        );
        match parse::send(ctx, HttpRequest::get(family, dyn_url)).await {
            Ok(dyn_res) => parse(&dyn_res),
            Err(verdict) => verdict,
        }
    })
}

async fn discover_package(ctx: &Context<Output>, family: IpFamily) -> Option<(u64, u64)> {
    let medias = ctx
        .probe(&HttpRequest::get(
            family,
            "https://capi.9c9media.com/destinations/crave_atexace/platforms/desktop/medias?%24top=1"
                .to_owned(),
        ))
        .await
        .ok()?;
    let mid = parse::json(&medias)?.pointer("/Items/0/Id")?.as_u64()?;
    let contents = ctx
        .probe(&HttpRequest::get(
            family,
            format!(
                "https://capi.9c9media.com/destinations/crave_atexace/platforms/desktop\
                 /medias/{mid}/contents?%24top=1"
            ),
        ))
        .await
        .ok()?;
    let cid = parse::json(&contents)?.pointer("/Items/0/Id")?.as_u64()?;
    let pkgs = ctx
        .probe(&HttpRequest::get(
            family,
            format!(
                "https://capi.9c9media.com/destinations/crave_atexace/platforms/desktop\
                 /contents/{cid}/contentpackages"
            ),
        ))
        .await
        .ok()?;
    let pid = parse::json(&pkgs)?.pointer("/Items/0/Id")?.as_u64()?;
    Some((cid, pid))
}

/// Parses Crave's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text();
    let lower = body.to_ascii_lowercase();
    if matches!(response.status, 403 | 451)
        || lower.contains("georestriction")
        || lower.contains("geo constraint")
    {
        return Verdict::no();
    }
    if response.status == 200
        && (body.contains("<MPD") || lower.contains("video.9c9media.com") || lower.contains("mpd"))
    {
        return Verdict::yes_in("CA");
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
