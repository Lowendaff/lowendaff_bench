//! Shodan InternetDB reconnaissance probe (`https://internetdb.shodan.io/<ip>`).

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
use super::super::result::ScoreVerdict;
use super::super::result::UsageType;
use super::ProbeFuture;
use super::Provider;

/// Provider specification for Shodan InternetDB.
pub const SPEC: Provider = Provider {
    id: "shodan",
    name: "Shodan",
    has_score: true,
    probe,
};

/// Probes Shodan InternetDB for `ip` (IPv4 dataset; IPv6 is not indexed by InternetDB).
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
        let real = format!("https://internetdb.shodan.io/{ip}");
        let url = parse::endpoint(base_url, SPEC.id, ip, &real);
        let res = parse::send_for_ip(ctx, primary, fallback, |fam| {
            HttpRequest::get(fam, url.clone())
        })
        .await?;
        parse(&res)
    })
}

/// Parses Shodan InternetDB's JSON payload.
pub fn parse(response: &HttpResponse) -> Result<ProviderData, ProviderFailure> {
    if let Some(early) = parse::gate(response) {
        return Err(early);
    }
    // HTTP 404 with JSON means Shodan observed no open ports or threat tags on this IPv4 address.
    if response.status == 404 {
        if parse::json(response).is_some_and(|val| val.is_object()) {
            return Ok(ProviderData {
                score: Some(ScoreVerdict::Shodan {
                    ports: 0,
                    vulns: 0,
                    flagged: false,
                }),
                ..ProviderData::default()
            });
        }
        return Err(ProviderFailure::Unexpected("HTTP 404".to_owned()));
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

    let ports = val
        .get("ports")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let vulns = val
        .get("vulns")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let hostname = val
        .get("hostnames")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(Value::as_str)
        .and_then(parse::clean_text);

    let tags: Vec<String> = val
        .get("tags")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_ascii_lowercase)
                .collect()
        })
        .unwrap_or_default();

    let has = |needle: &str| tags.iter().any(|t| t == needle);
    let vpn = has("vpn");
    let proxy = has("proxy");
    let tor = has("tor");
    let cloud = has("cloud");
    let cdn = has("cdn");
    let abused = has("compromised") || has("malware") || has("scanner");

    let mut flags = Vec::new();
    if vpn {
        flags.push((RiskFactor::Vpn, true));
    }
    if proxy {
        flags.push((RiskFactor::Proxy, true));
    }
    if tor {
        flags.push((RiskFactor::Tor, true));
    }
    if cloud {
        flags.push((RiskFactor::Datacenter, true));
    }
    if abused {
        flags.push((RiskFactor::Abuser, true));
    }

    let usage = if cdn {
        Some(UsageType::ContentDelivery)
    } else if cloud {
        Some(UsageType::Hosting)
    } else {
        None
    };

    Ok(ProviderData {
        hostname,
        usage,
        score: Some(ScoreVerdict::Shodan {
            ports,
            vulns,
            flagged: vpn || proxy || tor || abused,
        }),
        flags,
        ..ProviderData::default()
    })
}
