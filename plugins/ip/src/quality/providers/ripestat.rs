//! RIPEstat official RIR WHOIS registration probe across ARIN, RIPE, APNIC, LACNIC and AFRINIC.

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

/// Provider specification for RIPEstat RIR WHOIS.
pub const SPEC: Provider = Provider {
    id: "ripestat",
    name: "RIPEstat",
    has_score: false,
    probe,
};

/// Probes RIPEstat's WHOIS endpoint for `ip`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    ip: IpAddr,
    primary: IpFamily,
    fallback: Option<IpFamily>,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let real = format!("https://stat.ripe.net/data/whois/data.json?resource={ip}");
        let url = parse::endpoint(base_url, SPEC.id, ip, &real);
        let res = parse::send_for_ip(ctx, primary, fallback, |fam| {
            HttpRequest::get(fam, url.clone())
        })
        .await?;
        parse(&res)
    })
}

/// Parses RIPEstat's `whois/data.json` response.
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
    if let Some(status) = val.get("status").and_then(Value::as_str)
        && !status.eq_ignore_ascii_case("ok")
    {
        return Err(ProviderFailure::Unexpected(format!("status {status}")));
    }
    let Some(data) = val.get("data").filter(|d| d.is_object()) else {
        return Err(ProviderFailure::Unexpected(
            "missing data object".to_owned(),
        ));
    };

    let rir = data
        .get("authorities")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .find_map(parse::rir_name)
        });

    let mut registered_country = None;
    let mut org = None;
    let mut asn = None;

    if let Some(groups) = data.get("records").and_then(Value::as_array) {
        for group in groups {
            let Some(attrs) = group.as_array() else {
                continue;
            };
            for attr in attrs {
                let key = attr
                    .get("key")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let Some(raw) = attr.get("value").and_then(Value::as_str) else {
                    continue;
                };
                if key == "country" && registered_country.is_none() {
                    registered_country = parse::country_code(raw);
                } else if matches!(key.as_str(), "orgname" | "org-name" | "netname" | "owner")
                    && org.is_none()
                {
                    org = parse::clean_text(raw);
                } else if key == "origin" && asn.is_none() {
                    asn = parse::parse_asn(raw);
                }
            }
        }
    }

    if asn.is_none()
        && let Some(irr_groups) = data.get("irr_records").and_then(Value::as_array)
    {
        'irr: for group in irr_groups {
            let Some(attrs) = group.as_array() else {
                continue;
            };
            for attr in attrs {
                if attr
                    .get("key")
                    .and_then(Value::as_str)
                    .is_some_and(|k| k.eq_ignore_ascii_case("origin"))
                    && let Some(raw) = attr.get("value").and_then(Value::as_str)
                    && let Some(parsed) = parse::parse_asn(raw)
                {
                    asn = Some(parsed);
                    break 'irr;
                }
            }
        }
    }

    Ok(ProviderData {
        asn,
        org,
        rir,
        registered_country,
        ..ProviderData::default()
    })
}
