//! GreyNoise Community Internet-scanner telemetry probe (`https://api.greynoise.io/v3/community/`).

use std::net::IpAddr;

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use serde_json::Value;

use super::super::parse;
use super::super::parse::ProviderData;
use super::super::parse::ProviderFailure;
use super::super::result::GreyNoiseClass;
use super::super::result::Output;
use super::super::result::RiskFactor;
use super::super::result::ScoreVerdict;
use super::ProbeFuture;
use super::Provider;

/// Provider specification for GreyNoise Community.
pub const SPEC: Provider = Provider {
    id: "greynoise",
    name: "GreyNoise",
    has_score: true,
    probe,
};

/// Probes GreyNoise Community for `ip` (IPv4 space; IPv6 is not indexed by GreyNoise Community).
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    ip: IpAddr,
    primary: IpFamily,
    fallback: Option<IpFamily>,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        if ip.is_ipv6() && base_url.is_none() {
            return Err(ProviderFailure::NoAddress);
        }
        let real = format!("https://api.greynoise.io/v3/community/{ip}");
        let url = parse::endpoint(base_url, SPEC.id, ip, &real);
        let res = parse::send_for_ip(ctx, primary, fallback, |fam| {
            HttpRequest::get(fam, url.clone())
        })
        .await?;
        parse(&res)
    })
}

/// Parses GreyNoise Community's JSON payload.
pub fn parse(response: &HttpResponse) -> Result<ProviderData, ProviderFailure> {
    if let Some(early) = parse::gate(response) {
        return Err(early);
    }
    // HTTP 404 means GreyNoise has not observed this IP scanning the Internet.
    if response.status == 404 {
        return Ok(ProviderData {
            score: Some(ScoreVerdict::GreyNoise {
                riot: false,
                noise: false,
                classification: GreyNoiseClass::NotObserved,
            }),
            flags: vec![(RiskFactor::Abuser, false)],
            ..ProviderData::default()
        });
    }
    if response.status != 200 {
        return Err(ProviderFailure::Unexpected(format!(
            "HTTP {}",
            response.status
        )));
    }
    let Some(val) = parse::json(response) else {
        return Err(ProviderFailure::Unexpected("invalid JSON".to_owned()));
    };
    let noise = val.get("noise").and_then(Value::as_bool).unwrap_or(false);
    let riot = val.get("riot").and_then(Value::as_bool).unwrap_or(false);
    let class_str = val
        .get("classification")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let classification = match class_str.as_str() {
        "benign" => GreyNoiseClass::Benign,
        "malicious" => GreyNoiseClass::Malicious,
        _ if !noise && !riot => GreyNoiseClass::NotObserved,
        _ => GreyNoiseClass::Unknown,
    };

    Ok(ProviderData {
        score: Some(ScoreVerdict::GreyNoise {
            riot,
            noise,
            classification,
        }),
        flags: vec![(
            RiskFactor::Abuser,
            classification == GreyNoiseClass::Malicious,
        )],
        ..ProviderData::default()
    })
}
