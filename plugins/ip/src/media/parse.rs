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
    let mut found = None;
    for value in response.header_values("set-cookie") {
        let (pair, attrs) = value.split_once(';').unwrap_or((value, ""));
        let Some((key, raw_val)) = pair.split_once('=') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case(name) {
            continue;
        }
        let trimmed = raw_val.trim();
        let unquoted = trimmed
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or(trimmed)
            .trim();
        let expired = unquoted.is_empty()
            || attrs.split(';').any(|attr| {
                attr.split_once('=').is_some_and(|(k, v)| {
                    k.trim().eq_ignore_ascii_case("max-age")
                        && v.trim().parse::<i64>().is_ok_and(|secs| secs <= 0)
                })
            });
        if !expired {
            found = Some(unquoted.to_owned());
        }
    }
    found
}

/// Formats all non-expired `Set-Cookie` pairs set by `response` as a `Cookie` request header
/// (`"k1=v1; k2=v2"`), keeping the latest value when a cookie name repeats.
#[must_use]
pub fn cookies_header(response: &HttpResponse) -> Option<String> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for value in response.header_values("set-cookie") {
        let (pair, attrs) = value.split_once(';').unwrap_or((value, ""));
        let Some((raw_key, raw_val)) = pair.split_once('=') else {
            continue;
        };
        let key = raw_key.trim();
        if key.is_empty() {
            continue;
        }
        let val = raw_val.trim();
        let expired = val.is_empty()
            || attrs.split(';').any(|attr| {
                attr.split_once('=').is_some_and(|(k, v)| {
                    k.trim().eq_ignore_ascii_case("max-age")
                        && v.trim().parse::<i64>().is_ok_and(|secs| secs <= 0)
                })
            });
        if let Some(pos) = pairs.iter().position(|(k, _)| k.eq_ignore_ascii_case(key)) {
            if expired {
                pairs.remove(pos);
            } else {
                pairs[pos] = (key.to_owned(), val.to_owned());
            }
        } else if !expired {
            pairs.push((key.to_owned(), val.to_owned()));
        }
    }
    if pairs.is_empty() {
        None
    } else {
        let joined = pairs
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        Some(joined)
    }
}

/// Decodes standard or URL-safe Base64 (RFC 4648), ignoring ASCII whitespace and optional `=`
/// padding.
#[must_use]
pub fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0_u32;
    let mut bits = 0_u8;
    let mut pad_seen = false;
    for byte in input.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            pad_seen = true;
            continue;
        }
        if pad_seen {
            return None;
        }
        let sextet = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        buf = (buf << 6) | u32::from(sextet);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    if bits >= 6 {
        return None;
    }
    Some(out)
}

/// Decodes the JSON payload segment of a compact JWT (`header.payload.signature`) without
/// verifying its signature.
#[must_use]
pub fn jwt_payload(token: &str) -> Option<Value> {
    let mut segments = token.trim().split('.');
    let _header = segments.next()?;
    let payload = segments.next()?;
    let _sig = segments.next()?;
    if segments.next().is_some() {
        return None;
    }
    let decoded = base64_decode(payload)?;
    serde_json::from_slice(&decoded).ok()
}

/// Extracts the two-letter `loc=XX` country code from a Cloudflare `/cdn-cgi/trace` body.
#[must_use]
pub fn cf_trace_loc(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.trim().strip_prefix("loc="))
        .and_then(country_code)
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

/// Detects Cloudflare Turnstile, Cloudflare JS challenge, DataDome, PerimeterX, Imperva, and
/// Akamai anti-bot challenges.
#[must_use]
pub fn is_waf_challenge(response: &HttpResponse) -> bool {
    crate::waf::is_challenge(response)
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
    let clean = url_or_path
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let path = if let Some(scheme_end) = clean.find("://") {
        let after_host = &clean[scheme_end + 3..];
        let slash = after_host.find('/')?;
        &after_host[slash..]
    } else if let Some(after_slashes) = clean.strip_prefix("//") {
        let slash = after_slashes.find('/')?;
        &after_slashes[slash..]
    } else {
        clean
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
