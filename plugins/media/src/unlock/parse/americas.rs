//! Response parsers for the 10 North and Latin American streaming platforms.

use bench_api::HttpResponse;

use super::super::result::Restriction;
use super::super::result::Verdict;

/// Hulu (`hulu.com/welcome`).
#[must_use]
pub fn hulu(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location") {
        if loc.contains("hulu.jp") {
            return Verdict::yes_in("JP");
        }
        if loc.contains("unavailable") || loc.contains("geo") {
            return Verdict::no();
        }
    }
    let body = response.text();
    if body.contains("anonymous proxy tool") || body.contains("unblocker") {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if body.contains("not available in your region") {
        return Verdict::no();
    }
    if matches!(response.status, 200 | 301 | 302) {
        return Verdict::yes_in("US");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Max (`max.com/`).
#[must_use]
pub fn max(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location") {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("geo-availability") || lower.contains("unavailable") {
            return Verdict::no();
        }
        if let Some(code) = super::url_country(loc) {
            return Verdict::yes_in(code);
        }
    }
    let region = response
        .header("x-wbd-country")
        .and_then(super::country_code)
        .or_else(|| {
            super::extract_between(&response.text(), "\"countryCode\":\"", "\"")
                .and_then(super::country_code)
        });
    if matches!(response.status, 200 | 301 | 302 | 307 | 308) {
        return Verdict::Yes { region };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Peacock (`peacocktv.com/`).
#[must_use]
pub fn peacock(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location")
        && loc.to_ascii_lowercase().contains("unavailable")
    {
        return Verdict::no();
    }
    let territory = response
        .header("x-skyott-territory")
        .and_then(super::country_code)
        .unwrap_or_else(|| "US".to_owned());
    if matches!(response.status, 200 | 301 | 302) {
        return Verdict::yes_in(territory);
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Paramount+ (`paramountplus.com/`).
#[must_use]
pub fn paramount_plus(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location") {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("/intl") || lower.contains("unavailable") {
            return Verdict::no();
        }
        if let Some(code) = super::url_country(loc) {
            return Verdict::yes_in(code);
        }
    }
    if response.status == 200 {
        return Verdict::yes_in("US");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Starz (`auth.starz.com/api/v4/User/geolocation`).
#[must_use]
pub fn starz(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let allowed = val
            .get("isAllowedCountry")
            .and_then(serde_json::Value::as_bool);
        let vpn = val.get("isKnownProxy").and_then(serde_json::Value::as_bool);
        if vpn == Some(true) {
            return Verdict::No {
                restriction: Some(Restriction::ProxyDetected),
            };
        }
        let country = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        return match allowed {
            Some(true) => Verdict::Yes {
                region: Some(country.unwrap_or_else(|| "US".to_owned())),
            },
            Some(false) => Verdict::no(),
            None => Verdict::Unexpected("missing Starz isAllowedCountry".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// ESPN+ (`plus.espn.com/`).
#[must_use]
pub fn espn_plus(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location")
        && (loc.contains("unavailable") || loc.contains("block"))
    {
        return Verdict::no();
    }
    super::status_verdict(response, "US", &["not available in your region"])
}

/// Sling TV (`sling.com/`).
#[must_use]
pub fn sling_tv(response: &HttpResponse) -> Verdict {
    super::status_verdict(
        response,
        "US",
        &[
            "only available inside the us",
            "unavailable in your location",
        ],
    )
}

/// Pluto TV (`boot.pluto.tv/v4/start`).
#[must_use]
pub fn pluto_tv(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        if val.get("error").is_some() {
            return Verdict::no();
        }
        let country = val
            .pointer("/session/countryCode")
            .or_else(|| val.get("countryCode"))
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        if country.is_some() || val.get("sessionToken").is_some() {
            return Verdict::Yes { region: country };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Crave (`capi.9c9media.com/destinations/crave_web/...`).
#[must_use]
pub fn crave(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451)
        || body.contains(" GeoRestriction")
        || body.contains("geo")
    {
        return Verdict::no();
    }
    if matches!(response.status, 200 | 401) {
        return Verdict::yes_in("CA");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// CBC Gem (`services.radio-canada.ca/media/validation/v2/`).
#[must_use]
pub fn cbc_gem(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let error_code = val.get("errorCode").and_then(serde_json::Value::as_i64);
        let message = val
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if error_code == Some(0) || val.get("url").and_then(serde_json::Value::as_str).is_some() {
            return Verdict::yes_in("CA");
        }
        if error_code.is_some()
            || message.contains("not available")
            || message.contains("outside canada")
        {
            return Verdict::no();
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
