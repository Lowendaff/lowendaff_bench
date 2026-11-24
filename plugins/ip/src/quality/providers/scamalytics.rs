//! Scamalytics fraud-score and risk-factor probe (`https://scamalytics.com/ip/<ip>`).

use std::net::IpAddr;

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;

use super::super::parse;
use super::super::parse::ProviderData;
use super::super::parse::ProviderFailure;
use super::super::result::Output;
use super::super::result::RiskFactor;
use super::super::result::RiskLevel;
use super::super::result::ScoreVerdict;
use super::ProbeFuture;
use super::Provider;

/// Provider specification for Scamalytics.
pub const SPEC: Provider = Provider {
    id: "scamalytics",
    name: "Scamalytics",
    has_score: true,
    probe,
};

/// Probes Scamalytics for `ip`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    ip: IpAddr,
    primary: IpFamily,
    fallback: Option<IpFamily>,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let real = format!("https://scamalytics.com/ip/{ip}");
        let url = parse::endpoint(base_url, SPEC.id, ip, &real);
        let res = parse::send_for_ip(ctx, primary, fallback, |fam| {
            HttpRequest::get(fam, url.clone()).with_redirect(RedirectPolicy::Follow { max: 3 })
        })
        .await?;
        parse(&res)
    })
}

/// Parses Scamalytics's IP report page or JSON snippet.
pub fn parse(response: &HttpResponse) -> Result<ProviderData, ProviderFailure> {
    if let Some(early) = parse::gate(response) {
        return Err(early);
    }
    if matches!(response.status, 403 | 451) {
        return Err(ProviderFailure::BlockedWaf);
    }
    if response.status != 200 {
        return Err(ProviderFailure::Unexpected(format!(
            "HTTP {}",
            response.status
        )));
    }
    let body = response.text();
    let score = extract_score(&body)
        .ok_or_else(|| ProviderFailure::Unexpected("missing Scamalytics score".to_owned()))?;
    let level = extract_risk(&body).unwrap_or_else(|| parse::level_from_score(score));

    let mut flags = Vec::new();
    for (label, factor) in [
        ("Anonymizing VPN", RiskFactor::Vpn),
        ("Tor Exit Node", RiskFactor::Tor),
        ("Server", RiskFactor::Datacenter),
        ("Public Proxy", RiskFactor::Proxy),
        ("Web Proxy", RiskFactor::Proxy),
        ("Search Engine Robot", RiskFactor::Robot),
    ] {
        if let Some(hit) = extract_row_flag(&body, label) {
            if let Some(existing) = flags.iter_mut().find(|(f, _)| *f == factor) {
                existing.1 |= hit;
            } else {
                flags.push((factor, hit));
            }
        }
    }

    Ok(ProviderData {
        score: Some(ScoreVerdict::Numeric {
            score,
            max: 100,
            level,
        }),
        flags,
        ..ProviderData::default()
    })
}

fn extract_score(body: &str) -> Option<u8> {
    if let Some(raw) = parse::extract_between(body, "\"score\":\"", "\"")
        .or_else(|| parse::extract_between(body, "\"score\": \"", "\""))
        && let Ok(val) = raw.trim().parse::<u8>()
    {
        return Some(val.min(100));
    }
    let idx = body.find("Fraud Score:")?;
    let tail = &body[idx + "Fraud Score:".len()..];
    let digits: String = tail
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    let val: u8 = digits.parse().ok()?;
    Some(val.min(100))
}

fn extract_risk(body: &str) -> Option<RiskLevel> {
    let raw = parse::extract_between(body, "\"risk\":\"", "\"")
        .or_else(|| parse::extract_between(body, "\"risk\": \"", "\""))?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "low" => Some(RiskLevel::Low),
        "medium" => Some(RiskLevel::Medium),
        "high" => Some(RiskLevel::High),
        "very high" => Some(RiskLevel::VeryHigh),
        _ => None,
    }
}

fn extract_row_flag(body: &str, label: &str) -> Option<bool> {
    for (pos, _) in body.match_indices(label) {
        let after = &body[pos + label.len()..];
        if !body[..pos].trim_end().ends_with('>') || !after.trim_start().starts_with('<') {
            continue;
        }
        let bounded = &after[..after.floor_char_boundary(512)];
        let lower = bounded.to_ascii_lowercase();
        let row_end = [lower.find("</tr>"), lower.find("<tr>"), lower.find("<tr ")]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or_else(|| lower.floor_char_boundary(240));
        let row = &lower[..row_end];
        if row.contains(">yes<") || row.contains("\"risk yes\"") {
            return Some(true);
        }
        if row.contains(">no<") || row.contains("\"risk no\"") {
            return Some(false);
        }
    }
    None
}
