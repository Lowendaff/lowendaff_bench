//! Response parsers for the 12 Global streaming platforms.

use bench_api::HttpResponse;

use super::super::result::Restriction;
use super::super::result::Verdict;

/// Netflix: compares non-original (`/title/81280792`) and original (`/title/80018499`) answers.
#[must_use]
pub fn netflix(licensed: &HttpResponse, original: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(licensed) {
        return early;
    }
    if let Some(early) = super::gate(original) {
        return early;
    }
    let lic_state = netflix_page(licensed);
    let orig_state = netflix_page(original);
    let region = netflix_region(licensed).or_else(|| netflix_region(original));
    match (lic_state, orig_state) {
        (NetflixTitle::Available, _) => Verdict::Yes { region },
        (NetflixTitle::Blocked, NetflixTitle::Available) => Verdict::OriginalsOnly { region },
        (NetflixTitle::Proxy, _) | (_, NetflixTitle::Proxy) => Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        },
        (NetflixTitle::Blocked, NetflixTitle::Blocked) => Verdict::no(),
        (NetflixTitle::Unknown(status), _) | (_, NetflixTitle::Unknown(status)) => {
            Verdict::Unexpected(format!("HTTP {status}"))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NetflixTitle {
    Available,
    Blocked,
    Proxy,
    Unknown(u16),
}

fn netflix_page(response: &HttpResponse) -> NetflixTitle {
    let body = response.text();
    if body.contains("NSES-UH4") || body.contains("unblocker or proxy") {
        return NetflixTitle::Proxy;
    }
    if response.status == 403 || response.status == 404 {
        return NetflixTitle::Blocked;
    }
    if body.contains("Not Available")
        || body.contains("page-404")
        || response.final_url.contains("NotAvailable")
    {
        return NetflixTitle::Blocked;
    }
    if (200..300).contains(&response.status) {
        NetflixTitle::Available
    } else {
        NetflixTitle::Unknown(response.status)
    }
}

fn netflix_region(response: &HttpResponse) -> Option<String> {
    if let Some(code) = super::url_country(&response.final_url) {
        return Some(code);
    }
    let body = response.text();
    super::extract_between(&body, "\"requestCountry\":{\"id\":\"", "\"")
        .or_else(|| super::extract_between(&body, "\"country\":\"", "\""))
        .and_then(super::country_code)
}

/// YouTube Premium (`https://www.youtube.com/premium`).
#[must_use]
pub fn youtube(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if response.final_url.contains("google.cn") {
        return Verdict::No {
            restriction: Some(Restriction::ChinaMainland),
        };
    }
    if response.status != 200 {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    }
    let body = response.text();
    if body.contains("YouTube replicates") && body.contains("google.cn") {
        return Verdict::No {
            restriction: Some(Restriction::ChinaMainland),
        };
    }
    if body.contains("YouTube Premium is not available in your country")
        || body.contains("not available in your country")
    {
        return Verdict::no();
    }
    let region = super::extract_between(&body, "\"INNERTUBE_CONTEXT_GL\":\"", "\"")
        .or_else(|| super::extract_between(&body, "\"countryCode\":\"", "\""))
        .and_then(super::country_code);
    if region.is_some()
        || body.contains("YouTube and YouTube Music ad-free")
        || body.contains("Manage your membership")
        || body.contains("ytInitialData")
    {
        Verdict::Yes { region }
    } else {
        Verdict::Unexpected("unrecognized YouTube page".to_owned())
    }
}

/// Early check on Disney+'s root response before querying BAMGrid.
#[must_use]
pub fn disney_web_early(web: &HttpResponse) -> Option<Verdict> {
    if let Some(early) = super::gate(web) {
        return Some(early);
    }
    if let Some(loc) = web.header("location") {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("unavailable") || lower.contains("unsupported") {
            return Some(Verdict::no());
        }
        if lower.contains("preview") || lower.contains("coming-soon") {
            return Some(Verdict::No {
                restriction: Some(Restriction::ComingSoon),
            });
        }
    }
    if web.status == 403 || web.status == 451 {
        return Some(Verdict::no());
    }
    None
}

/// Disney+ combined web + BAMGrid evaluation.
#[must_use]
pub fn disney(web: &HttpResponse, api: Option<&HttpResponse>) -> Verdict {
    if let Some(early) = disney_web_early(web) {
        return early;
    }
    if let Some(api_res) = api
        && let Some(val) = super::json(api_res)
    {
        let session = val
            .pointer("/extensions/sdk/session")
            .or_else(|| val.pointer("/data/session"));
        if let Some(session) = session {
            let supported = session
                .pointer("/location/inSupportedLocation")
                .or_else(|| session.get("inSupportedLocation"))
                .and_then(serde_json::Value::as_bool);
            let country = session
                .pointer("/location/countryCode")
                .or_else(|| session.get("countryCode"))
                .and_then(serde_json::Value::as_str)
                .and_then(super::country_code);
            if supported == Some(false) {
                return Verdict::no();
            }
            if supported == Some(true) || country.is_some() {
                return Verdict::Yes { region: country };
            }
        }
    }
    let body = web.text();
    if body.contains("Disney+ is not available in your region")
        || body.contains("unavailable in your location")
    {
        return Verdict::no();
    }
    let region = web
        .header("location")
        .and_then(super::url_country)
        .or_else(|| {
            super::extract_between(&body, "\"countryCode\":\"", "\"").and_then(super::country_code)
        });
    if matches!(web.status, 200 | 301 | 302 | 307 | 308) {
        Verdict::Yes { region }
    } else {
        Verdict::Unexpected(format!("HTTP {}", web.status))
    }
}

/// Amazon Prime Video (`https://www.primevideo.com/`).
#[must_use]
pub fn prime_video(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if response.status != 200 {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    }
    let body = response.text();
    if body.contains("\"isServiceRestricted\":true") || body.contains("ServiceAreaRestriction") {
        return Verdict::no();
    }
    if body.contains("vpn") && body.contains("proxy") && body.contains("HTTPProxy") {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if let Some(region) =
        super::extract_between(&body, "\"currentTerritory\":\"", "\"").and_then(super::country_code)
    {
        return Verdict::Yes {
            region: Some(region),
        };
    }
    Verdict::Unexpected("missing currentTerritory".to_owned())
}

/// Apple TV+ (`https://tv.apple.com/`).
#[must_use]
pub fn apple_tv(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if body.contains("Apple TV+ is not available in your country")
        || response.final_url.contains("/unsupported")
    {
        return Verdict::no();
    }
    let region = super::url_country(&response.final_url).or_else(|| {
        super::extract_between(&body, "https://tv.apple.com/", "\"")
            .and_then(|tail| tail.split('/').next())
            .and_then(super::country_code)
    });
    if let Some(code) = region.as_deref()
        && matches!(code, "CN" | "CU" | "IR" | "KP" | "SY")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        Verdict::Yes { region }
    } else {
        Verdict::Unexpected(format!("HTTP {}", response.status))
    }
}

/// TikTok (`/cdn-cgi/trace` or region config).
#[must_use]
pub fn tiktok(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if response.status != 200 {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    }
    let body = response.text();
    let region = super::trace_loc(&body).or_else(|| {
        super::extract_between(&body, "\"region\":\"", "\"").and_then(super::country_code)
    });
    match region.as_deref() {
        Some("CN" | "HK" | "IN" | "IR" | "KP" | "AF") => Verdict::no(),
        Some(code) => Verdict::yes_in(code),
        None => Verdict::Unexpected("missing TikTok region".to_owned()),
    }
}

/// Spotify (`spclient.wg.spotify.com/signup/public/v1/account`).
#[must_use]
pub fn spotify(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let Some(val) = super::json(response) else {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    };
    let status = val.get("status").and_then(serde_json::Value::as_i64);
    let launched = val
        .get("is_country_launched")
        .and_then(serde_json::Value::as_bool);
    if status == Some(320) {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if launched == Some(false) || matches!(status, Some(311 | 120)) {
        return Verdict::no();
    }
    let country = val
        .get("country")
        .and_then(serde_json::Value::as_str)
        .and_then(super::country_code);
    if status == Some(1) || launched == Some(true) || country.is_some() {
        Verdict::Yes { region: country }
    } else {
        Verdict::Unexpected("unrecognized Spotify response".to_owned())
    }
}

/// DAZN (`startup.core.indazn.com/misl/v5/Startup`).
#[must_use]
pub fn dazn(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let Some(val) = super::json(response) else {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    };
    let region_obj = val.get("Region").or_else(|| val.get("region"));
    if let Some(reg) = region_obj {
        let allowed = reg
            .get("isAllowed")
            .or_else(|| reg.get("IsAllowed"))
            .and_then(serde_json::Value::as_bool);
        let country = reg
            .get("GeolocatedCountry")
            .or_else(|| reg.get("geolocatedCountry"))
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        return match allowed {
            Some(true) => Verdict::Yes { region: country },
            Some(false) => Verdict::no(),
            None => Verdict::Unexpected("missing DAZN isAllowed".to_owned()),
        };
    }
    Verdict::Unexpected("unrecognized DAZN payload".to_owned())
}

/// Crunchyroll (`crunchyroll.com/auth/v1/token`).
#[must_use]
pub fn crunchyroll(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451) || body.contains("forbidden_by_country") {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        if val.get("error").is_some() {
            return Verdict::no();
        }
        let country = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        if country.is_some() || val.get("access_token").is_some() {
            return Verdict::Yes { region: country };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Twitch (`gql.twitch.tv/gql`).
#[must_use]
pub fn twitch(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let country = val
            .pointer("/data/requestInfo/countryCode")
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        if let Some(code) = country.as_deref() {
            return if matches!(code, "CN" | "KP" | "RU" | "SK") {
                Verdict::no()
            } else {
                Verdict::yes_in(code)
            };
        }
        if response.status == 200 && val.get("data").is_some() {
            return Verdict::yes();
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Rakuten TV (`gizmo.rakuten.tv/v3/me/start`).
#[must_use]
pub fn rakuten_tv(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 404 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        if val.get("errors").is_some() {
            return Verdict::no();
        }
        let market = val
            .pointer("/data/market_code")
            .or_else(|| val.get("market_code"))
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        if market.is_some() || (response.status == 200 && val.get("data").is_some()) {
            return Verdict::Yes { region: market };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// MUBI (`api.mubi.com/v3/country`).
#[must_use]
pub fn mubi(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let available = val.get("available").and_then(serde_json::Value::as_bool);
        if available == Some(false) {
            return Verdict::no();
        }
        let code = val
            .get("code")
            .or_else(|| val.get("country"))
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        if matches!(code.as_deref(), Some("CN" | "KP" | "IR" | "SY" | "CU")) {
            return Verdict::no();
        }
        if code.is_some() || available == Some(true) {
            return Verdict::Yes { region: code };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
