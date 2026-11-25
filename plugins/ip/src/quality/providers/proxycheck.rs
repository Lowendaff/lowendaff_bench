//! ProxyCheck v2 risk score, VPN/proxy flag, and node classification probe.

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

/// Provider specification for ProxyCheck.
pub const SPEC: Provider = Provider {
    id: "proxycheck",
    name: "ProxyCheck",
    has_score: true,
    probe,
};

/// Probes ProxyCheck v2 for `ip`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    ip: IpAddr,
    primary: IpFamily,
    fallback: Option<IpFamily>,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let real = format!("https://proxycheck.io/v2/{ip}?vpn=1&asn=1&risk=1");
        let url = parse::endpoint(base_url, SPEC.id, ip, &real);
        let res = parse::send_for_ip(ctx, primary, fallback, |fam| {
            HttpRequest::get(fam, url.clone())
        })
        .await?;
        parse(&res)
    })
}

/// Parses ProxyCheck's JSON payload.
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
    let status = val
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if status.eq_ignore_ascii_case("denied") || status.eq_ignore_ascii_case("error") {
        let msg = val
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("denied");
        if msg.to_ascii_lowercase().contains("limit")
            || msg.to_ascii_lowercase().contains("queries")
        {
            return Err(ProviderFailure::RateLimited);
        }
        return Err(ProviderFailure::Unexpected(msg.to_owned()));
    }

    let Some(entry) = find_ip_entry(&val) else {
        return Err(ProviderFailure::Unexpected(
            "missing ProxyCheck IP object".to_owned(),
        ));
    };

    let asn = entry
        .get("asn")
        .and_then(Value::as_str)
        .and_then(parse::parse_asn);
    let org = entry
        .get("provider")
        .and_then(Value::as_str)
        .and_then(parse::clean_text)
        .or_else(|| {
            entry
                .get("organisation")
                .and_then(Value::as_str)
                .and_then(parse::clean_text)
        });
    let geo_country = entry
        .get("isocode")
        .and_then(Value::as_str)
        .and_then(parse::country_code);

    let proxy_yes = entry
        .get("proxy")
        .and_then(Value::as_str)
        .is_some_and(|s| s.eq_ignore_ascii_case("yes"));
    let kind = entry
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let lower_kind = kind.to_ascii_lowercase();

    let usage = if lower_kind == "hosting" {
        Some(UsageType::Hosting)
    } else if lower_kind == "residential" {
        Some(UsageType::Residential)
    } else if lower_kind == "business" {
        Some(UsageType::Business)
    } else if lower_kind == "wireless" {
        Some(UsageType::Mobile)
    } else {
        parse::usage_from_str(kind)
    };

    let vpn_yes = lower_kind.contains("vpn")
        || entry.get("vpn").is_some_and(|val| {
            val.as_bool() == Some(true)
                || val
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case("yes") || s == "1")
        });
    let mut flags = vec![
        (RiskFactor::Vpn, vpn_yes),
        (RiskFactor::Tor, lower_kind == "tor"),
        (
            RiskFactor::Proxy,
            proxy_yes && !vpn_yes && lower_kind != "tor",
        ),
    ];
    if usage == Some(UsageType::Hosting) {
        flags.push((RiskFactor::Datacenter, true));
    }
    if lower_kind.contains("compromised") {
        flags.push((RiskFactor::Abuser, true));
    }

    let score = entry
        .get("risk")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .and_then(|n| u8::try_from(n.min(100)).ok())
        .map(|num| ScoreVerdict::Numeric {
            score: num,
            max: 100,
            level: parse::level_from_score(num),
        });

    Ok(ProviderData {
        asn,
        org,
        geo_country,
        asn_usage: usage,
        score,
        flags,
        ..ProviderData::default()
    })
}

fn find_ip_entry(root: &Value) -> Option<&Value> {
    let map = root.as_object()?;
    map.iter()
        .find(|(key, val)| {
            !matches!(key.as_str(), "status" | "node" | "message" | "query time") && val.is_object()
        })
        .map(|(_, val)| val)
}
