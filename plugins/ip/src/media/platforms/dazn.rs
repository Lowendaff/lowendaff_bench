//! DAZN `Startup` geolocation probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for DAZN.
pub const SPEC: Platform = Platform {
    id: "dazn",
    name: "DAZN",
    probe,
};

/// Probes DAZN over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::post(
            family,
            url("https://startup.core.indazn.com/misl/v5/Startup"),
            concat!(
                r#"{"LandingPageKey":"generic","Languages":"en-US,en","#,
                r#""Platform":"web","Manufacturer":"","PromoCode":""}"#
            )
            .as_bytes()
            .to_vec(),
        )
        .with_header("content-type", "application/json");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses DAZN's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let Some(val) = parse::json(response) else {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    };
    let region_obj = val.get("Region").or_else(|| val.get("region"));
    if let Some(reg) = region_obj {
        let allowed = reg
            .get("isAllowed")
            .or_else(|| reg.get("IsAllowed"))
            .and_then(serde_json::Value::as_bool);
        let country = reg
            .get("GeolocatedCountry")
            .or_else(|| reg.get("geolocatedCountry"))
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code);
        return match allowed {
            Some(true) => Verdict::Yes { region: country },
            Some(false) => Verdict::no(),
            None => Verdict::Unexpected("missing DAZN isAllowed".to_owned()),
        };
    }
    Verdict::Unexpected("unrecognized DAZN payload".to_owned())
}
