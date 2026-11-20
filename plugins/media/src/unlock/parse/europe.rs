//! Response parsers for the 14 European streaming platforms.

use bench_api::HttpResponse;

use super::super::result::Restriction;
use super::super::result::Verdict;

/// BBC iPlayer (`open.live.bbc.co.uk/mediaselector/6/.../bbc_one_london`).
#[must_use]
pub fn bbc_iplayer(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    let body = response.text();
    if body.contains("geolocation") {
        return Verdict::no();
    }
    if body.contains("vpn") {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if response.status == 200 && body.contains("\"media\"") {
        return Verdict::yes_in("GB");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Channel 4 (`channel4.com/simulcast/channels/C4`).
#[must_use]
pub fn channel_4(response: &HttpResponse) -> Verdict {
    super::status_verdict(
        response,
        "GB",
        &["not available in your location", "geo-blocked"],
    )
}

/// ITVX (`simulcast.itv.com/playlist/itvonline/ITV`).
#[must_use]
pub fn itv_x(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    match response.status {
        // 200 or 401 (domestic auth required, passed geo gate) means GB unlocked; 403 is geo-block.
        200 | 401 => Verdict::yes_in("GB"),
        403 | 451 => Verdict::no(),
        status => Verdict::Unexpected(format!("HTTP {status}")),
    }
}

/// Sky Go (`awk.epgsky.com`).
#[must_use]
pub fn sky_go(response: &HttpResponse) -> Verdict {
    super::status_verdict(response, "GB", &["access denied", "geoblocked"])
}

/// ZDF (`ssl.zdf.de/geo/de/geo.txt`).
#[must_use]
pub fn zdf(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if response.status == 200 {
        let code = super::country_code(&response.text()).unwrap_or_else(|| "DE".to_owned());
        return Verdict::yes_in(code);
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Joyn (`api.joyn.de`).
#[must_use]
pub fn joyn(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if body.contains("not available in your country") || body.contains("GeoBlocked") {
        return Verdict::no();
    }
    if matches!(response.status, 200 | 404) {
        return Verdict::yes_in("DE");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// RTL+ (`plus.rtl.de`).
#[must_use]
pub fn rtl_plus(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if response.final_url.contains("geoblocking") || matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    super::status_verdict(
        response,
        "DE",
        &["geoblocking", "not available in your country"],
    )
}

/// Canal+ (`boutique.canalplus.com`).
#[must_use]
pub fn canal_plus(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location")
        && (loc.contains("unavailable") || loc.contains("geo"))
    {
        return Verdict::no();
    }
    if matches!(response.status, 200 | 301 | 302) {
        return Verdict::yes_in("FR");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Molotov (`fapi.molotov.tv/v2/startup/config`).
#[must_use]
pub fn molotov(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let is_fr = val
            .get("is_france")
            .or_else(|| val.pointer("/geo/is_france"))
            .and_then(serde_json::Value::as_bool);
        let country = val
            .get("country")
            .or_else(|| val.pointer("/geo/country"))
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        return match (is_fr, country.as_deref()) {
            (Some(true), _) | (None, Some("FR")) => Verdict::yes_in("FR"),
            (Some(false), _) | (None, Some(_)) => Verdict::no(),
            (None, None) if response.status == 200 => Verdict::yes_in("FR"),
            _ => Verdict::Unexpected("unrecognized Molotov payload".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// france.tv (`k7.ftven.fr/videos/...`).
#[must_use]
pub fn france_tv(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451)
        || body.contains("geoblocage")
        || body.contains("\"code\":2003")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("FR");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// RaiPlay (`mediapolisvod.rai.it/relinker/relinkerServlet.htm`).
#[must_use]
pub fn rai_play(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451)
        || body.contains("video_no_available")
        || body.contains("geoprotetto")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("IT");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Movistar Plus+ (`contratar.movistarplus.es`).
#[must_use]
pub fn movistar_plus(response: &HttpResponse) -> Verdict {
    super::status_verdict(
        response,
        "ES",
        &["fuera de espana", "no disponible en tu pais"],
    )
}

/// NLZIET (`app.nlziet.nl`).
#[must_use]
pub fn nlziet(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location")
        && loc.contains("geo")
    {
        return Verdict::no();
    }
    if matches!(response.status, 200 | 301 | 302) {
        return Verdict::yes_in("NL");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Viaplay (`checkout.viaplay.com`).
#[must_use]
pub fn viaplay(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location") {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("region-blocked") || lower.contains("unavailable") {
            return Verdict::no();
        }
        if let Some(code) = super::url_country(loc) {
            return Verdict::yes_in(code);
        }
    }
    if response.status == 200 {
        let code = super::url_country(&response.final_url);
        return Verdict::Yes { region: code };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
