//! `ipapi.is` intelligence probe: RIR, ASN, company/ASN abuser score, usage type, GeoIP, and
//! security flags (`is_proxy`, `is_vpn`, `is_tor`, `is_datacenter`, `is_abuser`, `is_crawler`,
//! `is_bogon`).

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
use super::super::result::RiskLevel;
use super::super::result::ScoreVerdict;
use super::super::result::UsageType;
use super::ProbeFuture;
use super::Provider;

/// Provider specification for `ipapi.is`.
pub const SPEC: Provider = Provider {
    id: "ipapi",
    name: "ipapi.is",
    has_score: true,
    probe,
};

/// Probes `ipapi.is` for `ip`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    ip: IpAddr,
    primary: IpFamily,
    fallback: Option<IpFamily>,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let real = format!("https://api.ipapi.is/?q={ip}");
        let url = parse::endpoint(base_url, SPEC.id, ip, &real);
        let res = parse::send_for_ip(ctx, primary, fallback, |fam| {
            HttpRequest::get(fam, url.clone())
        })
        .await?;
        parse(&res)
    })
}

/// Parses `ipapi.is`'s JSON payload.
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
    if let Some(err) = val.get("error").and_then(Value::as_str) {
        if err.to_ascii_lowercase().contains("limit") {
            return Err(ProviderFailure::RateLimited);
        }
        return Err(ProviderFailure::Unexpected(err.to_owned()));
    }

    let rir = val
        .get("rir")
        .and_then(Value::as_str)
        .and_then(parse::rir_name);
    let asn_obj = val.get("asn");
    let company_obj = val.get("company");
    let asn = asn_obj
        .and_then(|obj| obj.get("asn"))
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&n| n > 0);
    let org = asn_obj
        .and_then(|obj| {
            obj.get("org")
                .and_then(Value::as_str)
                .and_then(parse::clean_text)
                .or_else(|| {
                    obj.get("descr")
                        .and_then(Value::as_str)
                        .and_then(parse::clean_text)
                })
        })
        .or_else(|| {
            company_obj
                .and_then(|obj| obj.get("name"))
                .and_then(Value::as_str)
                .and_then(parse::clean_text)
        });
    let registered_country = asn_obj
        .and_then(|obj| obj.get("country"))
        .and_then(Value::as_str)
        .and_then(parse::country_code);
    let geo_country = val
        .pointer("/location/country_code")
        .and_then(Value::as_str)
        .and_then(parse::country_code);
    let city = val
        .pointer("/location/city")
        .and_then(Value::as_str)
        .and_then(parse::clean_place);
    let region = val
        .pointer("/location/state")
        .and_then(Value::as_str)
        .and_then(parse::clean_place);
    let timezone = val
        .pointer("/location/timezone")
        .and_then(Value::as_str)
        .and_then(parse::clean_timezone);

    let company_usage = company_obj
        .and_then(|obj| obj.get("type"))
        .and_then(Value::as_str)
        .and_then(parse::usage_from_str);
    let mut asn_usage = asn_obj
        .and_then(|obj| obj.get("type"))
        .and_then(Value::as_str)
        .and_then(parse::usage_from_str);
    if asn_usage.is_none() && company_usage.is_none() {
        if val.get("is_mobile").and_then(Value::as_bool) == Some(true) {
            asn_usage = Some(UsageType::Mobile);
        } else if val.get("is_datacenter").and_then(Value::as_bool) == Some(true) {
            asn_usage = Some(UsageType::Hosting);
        }
    }
    let effective_usage = company_usage.or(asn_usage);

    let score = company_obj
        .and_then(|obj| obj.get("abuser_score"))
        .and_then(Value::as_str)
        .and_then(parse_abuser_score)
        .or_else(|| {
            asn_obj
                .and_then(|obj| obj.get("abuser_score"))
                .and_then(Value::as_str)
                .and_then(parse_abuser_score)
        });

    let mut flags = Vec::new();
    for (field, factor) in [
        ("is_proxy", RiskFactor::Proxy),
        ("is_vpn", RiskFactor::Vpn),
        ("is_tor", RiskFactor::Tor),
        ("is_datacenter", RiskFactor::Datacenter),
        ("is_abuser", RiskFactor::Abuser),
        ("is_crawler", RiskFactor::Robot),
        ("is_bogon", RiskFactor::Bogon),
    ] {
        if let Some(flag) = val.get(field).and_then(Value::as_bool) {
            flags.push((factor, flag));
        }
    }
    if !flags.iter().any(|(f, _)| *f == RiskFactor::Datacenter)
        && effective_usage == Some(UsageType::Hosting)
    {
        flags.push((RiskFactor::Datacenter, true));
    }

    Ok(ProviderData {
        asn,
        org,
        hostname: None,
        rir,
        registered_country,
        geo_country,
        city,
        region,
        timezone,
        asn_usage,
        company_usage,
        score,
        flags,
    })
}

fn parse_abuser_score(raw: &str) -> Option<ScoreVerdict> {
    let trimmed = raw.trim();
    let ratio_part = trimmed.split_whitespace().next()?;
    let numeric: f64 = ratio_part.parse().ok()?;
    if !numeric.is_finite() || numeric < 0.0 || ratio_part.starts_with('-') {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let level = if lower.contains("very high") {
        RiskLevel::VeryHigh
    } else if lower.contains("high") {
        RiskLevel::High
    } else if lower.contains("elevated") || lower.contains("medium") {
        RiskLevel::Medium
    } else if lower.contains("low") {
        RiskLevel::Low
    } else if numeric >= 0.20 {
        RiskLevel::VeryHigh
    } else if numeric >= 0.05 {
        RiskLevel::High
    } else if numeric >= 0.01 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };
    Some(ScoreVerdict::Ratio {
        text: ratio_part.to_owned(),
        level,
    })
}
