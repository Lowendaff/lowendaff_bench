//! IPWHOIS (`https://ipwho.is/<ip>`) GeoIP and ASN/ISP cross-check probe.

use std::net::IpAddr;

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use serde_json::Value;

use super::super::parse;
use super::super::parse::ProviderData;
use super::super::parse::ProviderFailure;
use super::super::result::Output;
use super::ProbeFuture;
use super::Provider;

/// Provider specification for IPWHOIS.
pub const SPEC: Provider = Provider {
    id: "ipwhois",
    name: "IPWHOIS",
    has_score: false,
    probe,
};

/// Probes `ipwho.is` for `ip`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    ip: IpAddr,
    primary: IpFamily,
    fallback: Option<IpFamily>,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let real = format!("https://ipwho.is/{ip}");
        let url = parse::endpoint(base_url, SPEC.id, ip, &real);
        let res = parse::send_for_ip(ctx, primary, fallback, |fam| {
            HttpRequest::get(fam, url.clone())
        })
        .await?;
        parse(&res)
    })
}

/// Parses `ipwho.is`'s JSON payload.
pub fn parse(response: &HttpResponse) -> Result<ProviderData, ProviderFailure> {
    if let Some(early) = parse::gate(response) {
        return Err(early);
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
    if val.get("success").and_then(Value::as_bool) == Some(false) {
        let msg = val
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("request rejected");
        if msg.to_ascii_lowercase().contains("limit") {
            return Err(ProviderFailure::RateLimited);
        }
        return Err(ProviderFailure::Unexpected(msg.to_owned()));
    }

    let geo_country = val
        .get("country_code")
        .and_then(Value::as_str)
        .and_then(parse::country_code);
    let city = val
        .get("city")
        .and_then(Value::as_str)
        .and_then(parse::clean_place);
    let region = val
        .get("region")
        .and_then(Value::as_str)
        .and_then(parse::clean_place);
    let timezone = val
        .pointer("/timezone/id")
        .and_then(Value::as_str)
        .and_then(parse::clean_timezone);
    let conn = val.get("connection");
    let asn = conn
        .and_then(|c| c.get("asn"))
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&n| n > 0);
    let org = conn.and_then(|c| {
        c.get("org")
            .and_then(Value::as_str)
            .and_then(parse::clean_text)
            .or_else(|| {
                c.get("isp")
                    .and_then(Value::as_str)
                    .and_then(parse::clean_text)
            })
    });

    Ok(ProviderData {
        asn,
        org,
        geo_country,
        city,
        region,
        timezone,
        ..ProviderData::default()
    })
}
