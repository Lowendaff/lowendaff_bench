//! DB-IP free GeoIP cross-check probe (`https://api.db-ip.com/v2/free/<ip>`).

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

/// Provider specification for DB-IP.
pub const SPEC: Provider = Provider {
    id: "dbip",
    name: "DB-IP",
    has_score: false,
    probe,
};

/// Probes DB-IP's free endpoint for `ip`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    ip: IpAddr,
    primary: IpFamily,
    fallback: Option<IpFamily>,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let real = format!("https://api.db-ip.com/v2/free/{ip}");
        let url = parse::endpoint(base_url, SPEC.id, ip, &real);
        let res = parse::send_for_ip(ctx, primary, fallback, |fam| {
            HttpRequest::get(fam, url.clone())
        })
        .await?;
        parse(&res)
    })
}

/// Parses DB-IP's JSON payload.
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
    if let Some(err) = val.get("errorCode").and_then(Value::as_str) {
        if err.contains("OVER_QUERY_LIMIT") || err.contains("TEMPORARY_BLOCKED") {
            return Err(ProviderFailure::RateLimited);
        }
        return Err(ProviderFailure::Unexpected(err.to_owned()));
    }
    let Some(raw_country) = val
        .get("countryCode")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|code| code.len() == 2 && code.bytes().all(|b| b.is_ascii_alphabetic()))
    else {
        return Err(ProviderFailure::Unexpected(
            "missing DB-IP countryCode".to_owned(),
        ));
    };

    Ok(ProviderData {
        geo_country: parse::country_code(raw_country),
        ..ProviderData::default()
    })
}
