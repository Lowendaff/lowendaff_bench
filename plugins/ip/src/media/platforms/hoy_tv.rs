//! HOY TV Hong Kong channel availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for HOY TV.
pub const SPEC: Platform = Platform {
    id: "hoy-tv",
    name: "HOY TV",
    probe,
};

/// Probes HOY TV over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://api2.hoy.tv/api/v3/a/channel"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses HOY TV's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let blocked = val
            .get("code")
            .is_some_and(|code| code.as_i64() == Some(403) || code.as_str() == Some("403"));
        if blocked {
            return Verdict::no();
        }
    }
    let body = response.text().to_ascii_lowercase();
    if body.contains("geoblock")
        || body.contains("geo-block")
        || body.contains("geo_block")
        || body.contains("geolocation")
        || body.contains("georestrict")
        || body.contains("not available")
        || body.contains("outside hong kong")
        || body.contains("\"code\":403")
    {
        return Verdict::no();
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
