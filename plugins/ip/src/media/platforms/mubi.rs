//! MUBI storefront redirect and country availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for MUBI.
pub const SPEC: Platform = Platform {
    id: "mubi",
    name: "MUBI",
    probe,
};

/// Probes MUBI over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://mubi.com/"))
            .with_redirect(RedirectPolicy::Follow { max: 5 });
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses MUBI's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let available = val.get("available").and_then(serde_json::Value::as_bool);
        if available == Some(false) {
            return Verdict::no();
        }
        let field_code = |key: &str| {
            val.get(key)
                .and_then(serde_json::Value::as_str)
                .and_then(parse::country_code)
        };
        let code = field_code("code").or_else(|| field_code("country"));
        if matches!(code.as_deref(), Some("CN" | "KP" | "IR" | "SY" | "CU")) {
            return Verdict::no();
        }
        if (200..300).contains(&response.status) && (code.is_some() || available == Some(true)) {
            return Verdict::Yes { region: code };
        }
    }
    if (200..300).contains(&response.status) {
        let body = response.text();
        if body
            .to_ascii_lowercase()
            .contains("not available in your country")
        {
            return Verdict::no();
        }
        if let Some(code) = mubi_url_country(&response.final_url).or_else(|| {
            parse::extract_between(&body, "\"country\":\"", "\"").and_then(parse::country_code)
        }) {
            if matches!(code.as_str(), "CN" | "KP" | "IR" | "SY" | "CU") {
                return Verdict::no();
            }
            return Verdict::yes_in(code);
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Extracts `<country>` from MUBI storefront URLs such as `https://mubi.com/en/hk` or
/// `https://mubi.com/de`.
fn mubi_url_country(raw_url: &str) -> Option<String> {
    let clean = raw_url.trim().split(['?', '#']).next().unwrap_or_default();
    let after_scheme = clean.split_once("://").map_or(clean, |(_, rest)| rest);
    let path = after_scheme
        .find('/')
        .map_or("", |slash| &after_scheme[slash..]);
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        [lang, country, ..] if (2..=3).contains(&lang.len()) && country.len() == 2 => {
            parse::country_code(country)
        }
        [country] if country.len() == 2 && !country.eq_ignore_ascii_case("en") => {
            parse::country_code(country)
        }
        _ => None,
    }
}
