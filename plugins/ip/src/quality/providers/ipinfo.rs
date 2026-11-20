//! IPinfo unauthenticated GeoIP, ASN/organization, and reverse-hostname probe.

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
use super::super::result::RiskFactor;
use super::ProbeFuture;
use super::Provider;

/// Provider specification for IPinfo.
pub const SPEC: Provider = Provider {
    id: "ipinfo",
    name: "IPinfo",
    has_score: false,
    probe,
};

/// Probes IPinfo for `ip`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    ip: IpAddr,
    primary: IpFamily,
    fallback: Option<IpFamily>,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let real = format!("https://ipinfo.io/{ip}/json");
        let url = parse::endpoint(base_url, SPEC.id, ip, &real);
        let res = parse::send_for_ip(ctx, primary, fallback, |fam| {
            HttpRequest::get(fam, url.clone()).with_header("accept", "application/json")
        })
        .await?;
        parse(&res)
    })
}

/// Parses IPinfo's JSON payload.
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

    let geo_country = val
        .get("country")
        .and_then(Value::as_str)
        .and_then(parse::country_code);
    let hostname = val
        .get("hostname")
        .and_then(Value::as_str)
        .and_then(parse::clean_text);

    let (asn, org) = match val.get("org").and_then(Value::as_str) {
        Some(raw) => split_asn_org(raw),
        None => (None, None),
    };

    let mut flags = Vec::new();
    if let Some(bogon) = val.get("bogon").and_then(Value::as_bool) {
        flags.push((RiskFactor::Bogon, bogon));
    }

    Ok(ProviderData {
        asn,
        org,
        hostname,
        geo_country,
        flags,
        ..ProviderData::default()
    })
}

fn split_asn_org(raw: &str) -> (Option<u32>, Option<String>) {
    let trimmed = raw.trim();
    if let Some((first, rest)) = trimmed.split_once(' ')
        && let Some(asn) = parse::parse_asn(first)
    {
        return (Some(asn), parse::clean_text(rest));
    }
    if let Some(asn) = parse::parse_asn(trimmed) {
        return (Some(asn), None);
    }
    (None, parse::clean_text(trimmed))
}
