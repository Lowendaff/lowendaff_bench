//! Pure response parsers for all 68 streaming platforms, plus shared WAF, rate-limit, JSON and
//! ISO 3166 region-code helpers.

pub mod americas;
pub mod asia;
pub mod europe;
pub mod global;
pub mod oceania;

use bench_api::HttpError;
use bench_api::HttpResponse;
use serde_json::Value;

use super::result::Restriction;
use super::result::Verdict;

/// Maps an [`HttpError`] to the corresponding [`Verdict`].
#[must_use]
pub fn error_verdict(error: &HttpError) -> Verdict {
    match error {
        HttpError::NoAddress => Verdict::NoAddress,
        HttpError::TimedOut => Verdict::TimedOut,
        HttpError::TooManyRedirects => Verdict::Unexpected("too many redirects".to_owned()),
        HttpError::InvalidRedirect(why) => Verdict::Unexpected(format!("invalid redirect: {why}")),
        HttpError::NotHttps(_) => Verdict::Unexpected("non-HTTPS endpoint".to_owned()),
        HttpError::Transport(why) => Verdict::Unexpected(why.clone()),
        HttpError::Cancelled => Verdict::Unexpected("interrupted".to_owned()),
    }
}

/// Checks universal rate-limit (`429`), anti-bot/WAF challenge, and `5xx` server errors before a
/// service's own rules run. Never attempts to solve or bypass a challenge.
#[must_use]
pub fn gate(response: &HttpResponse) -> Option<Verdict> {
    if response.status == 429 {
        return Some(Verdict::RateLimited);
    }
    if is_waf_challenge(response) {
        return Some(Verdict::BlockedWaf);
    }
    if response.status >= 500 {
        return Some(Verdict::Unexpected(format!("HTTP {}", response.status)));
    }
    None
}

/// Detects Cloudflare Turnstile, Cloudflare JS challenge, DataDome, PerimeterX and Imperva
/// anti-bot challenges.
#[must_use]
pub fn is_waf_challenge(response: &HttpResponse) -> bool {
    if response
        .header("cf-mitigated")
        .is_some_and(|val| val.eq_ignore_ascii_case("challenge"))
    {
        return true;
    }
    if response.header("x-datadome").is_some() {
        return true;
    }
    if !matches!(response.status, 403 | 503) {
        return false;
    }
    let body = response.text();
    let markers = [
        "challenges.cloudflare.com/turnstile",
        "/cdn-cgi/challenge-platform/",
        "<title>Just a moment...</title>",
        "Attention Required! | Cloudflare",
        "_Incapsula_Resource",
        "px-captcha",
    ];
    markers.iter().any(|marker| body.contains(marker))
}

/// Parses the response body as JSON.
#[must_use]
pub fn json(response: &HttpResponse) -> Option<Value> {
    serde_json::from_slice(&response.body).ok()
}

/// Normalizes a two-letter ASCII country code (`"us"` -> `"US"`), rejecting anything else.
#[must_use]
pub fn country_code(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.len() == 2 && trimmed.bytes().all(|b| b.is_ascii_alphabetic()) {
        let upper = trimmed.to_ascii_uppercase();
        if upper != "XX" && upper != "ZZ" {
            return Some(upper);
        }
    }
    None
}

/// Extracts a two-letter country prefix from a URL path segment such as `/us/title/...`,
/// `/hk-en/title/...` or `https://example.com/ott/sg/index`.
#[must_use]
pub fn url_country(url_or_path: &str) -> Option<String> {
    let path = match url_or_path.find("://") {
        Some(scheme_end) => {
            let after_host = &url_or_path[scheme_end + 3..];
            let slash = after_host.find('/')?;
            &after_host[slash..]
        }
        None => url_or_path,
    };
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        let code_part = match segment.split_once('-') {
            Some((cc, lang))
                if cc.len() == 2
                    && (2..=4).contains(&lang.len())
                    && lang.bytes().all(|b| b.is_ascii_alphabetic()) =>
            {
                cc
            }
            None if segment.len() == 2 => segment,
            _ => continue,
        };
        if let Some(code) = country_code(code_part) {
            return Some(code);
        }
    }
    None
}

/// Extracts the value after `prefix` up to `quote`, e.g. `"currentTerritory":"US"`.
#[must_use]
pub fn extract_between<'a>(haystack: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = haystack.find(prefix)? + prefix.len();
    let rest = &haystack[start..];
    let end = rest.find(suffix)?;
    Some(&rest[..end])
}

/// Extracts `loc=XX` from a Cloudflare `/cdn-cgi/trace` body.
#[must_use]
pub fn trace_loc(body: &str) -> Option<String> {
    for line in body.lines() {
        if let Some(value) = line.strip_prefix("loc=") {
            return country_code(value);
        }
    }
    None
}

/// Verdict for a simple region-locked JSON or HTTP endpoint where `200` means unlocked in `region`
/// and `401`/`403`/`451` means geo-blocked (`No`), unless body markers indicate proxy or geo block.
#[must_use]
pub fn status_verdict(response: &HttpResponse, region: &str, blocked_markers: &[&str]) -> Verdict {
    if let Some(early) = gate(response) {
        return early;
    }
    let body = response.text();
    let lower = body.to_ascii_lowercase();
    if lower.contains("vpn") && (lower.contains("proxy") || lower.contains("unblocker")) {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    for marker in blocked_markers {
        if lower.contains(&marker.to_ascii_lowercase()) {
            return Verdict::no();
        }
    }
    match response.status {
        200..=299 => Verdict::yes_in(region),
        403 | 451 => Verdict::no(),
        status => Verdict::Unexpected(format!("HTTP {status}")),
    }
}
