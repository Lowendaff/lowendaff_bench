//! Shared HTTP probe helpers and pure response-inspection utilities used by platform modules.

use bench_api::Context;
use bench_api::HttpError;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use serde_json::Value;

use super::result::Output;
use super::result::Restriction;
use super::result::Verdict;

/// Resolves the target URL: `base_url/<id>/<step>` when `base_url` is set, else `real_url`.
#[must_use]
pub fn endpoint(base_url: Option<&str>, id: &str, step: &str, real_url: &str) -> String {
    match base_url {
        Some(base) => format!("{base}/{id}/{step}"),
        None => real_url.to_owned(),
    }
}

/// Sends `request` through `ctx`, converting transport errors into [`Verdict`]s.
pub async fn send(ctx: &Context<Output>, request: HttpRequest) -> Result<HttpResponse, Verdict> {
    ctx.probe(&request).await.map_err(|err| error_verdict(&err))
}

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

/// Verdict for an endpoint that answered without naming a region or a restriction.
#[must_use]
pub fn no_region() -> Verdict {
    Verdict::Unexpected("endpoint reports no region".to_owned())
}

/// Value of cookie `name` set by `response`, if any.
#[must_use]
pub fn cookie(response: &HttpResponse, name: &str) -> Option<String> {
    response
        .header_values("set-cookie")
        .into_iter()
        .find_map(|value| {
            let (key, rest) = value.split_once('=')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then(|| rest.split(';').next().unwrap_or_default().trim().to_owned())
        })
}

/// Checks universal rate-limit (`429`), anti-bot/WAF challenge, and `5xx` server errors before a
/// platform's own rules run. Never attempts to solve or bypass a challenge.
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

/// Detects Cloudflare Turnstile, Cloudflare JS challenge, DataDome, PerimeterX, Imperva and
/// Akamai anti-bot challenges.
#[must_use]
pub fn is_waf_challenge(response: &HttpResponse) -> bool {
    if response
        .header("cf-mitigated")
        .is_some_and(|val| val.eq_ignore_ascii_case("challenge"))
    {
        return true;
    }
    if !matches!(response.status, 403 | 503) {
        return false;
    }
    if response.header("x-dd-b").is_some() {
        return true;
    }
    let body = response.text().to_ascii_lowercase();
    let markers = [
        "challenges.cloudflare.com/turnstile",
        "/cdn-cgi/challenge-platform/",
        "<title>just a moment...</title>",
        "attention required! | cloudflare",
        "_incapsula_resource",
        "px-captcha",
        "captcha-delivery.com",
        "errors.edgesuite.net",
        "errors&#46;edgesuite&#46;net",
    ];
    markers.iter().any(|marker| body.contains(marker))
}

/// Parses the response body as JSON.
#[must_use]
pub fn json(response: &HttpResponse) -> Option<Value> {
    serde_json::from_slice(&response.body).ok()
}

/// Normalizes a two-letter ASCII country code (`"us"` -> `"US"`), rejecting placeholders.
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

/// Extracts the substring between `prefix` and `suffix`.
#[must_use]
pub fn extract_between<'a>(haystack: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = haystack.find(prefix)? + prefix.len();
    let rest = &haystack[start..];
    let end = rest.find(suffix)?;
    Some(&rest[..end])
}

/// Verdict for a region-locked endpoint where `2xx` means unlocked in `region` and `403`/`451`
/// means geo-blocked, unless `blocked_markers` or proxy markers appear in the body.
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
