//! Response parsers for the 24 Asian streaming platforms (HK, MO, TW, JP, KR, SG).

use bench_api::HttpResponse;

use super::super::result::Verdict;

/// Bilibili PGC `playurl`: compares Taiwan-only (`tw`) and HK/MO/TW (`hmt`) bangumi answers.
#[must_use]
pub fn bilibili(tw: &HttpResponse, hmt: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(tw) {
        return early;
    }
    if let Some(early) = super::gate(hmt) {
        return early;
    }
    let tw_code = bilibili_code(tw);
    let hmt_code = bilibili_code(hmt);
    match (tw_code, hmt_code) {
        (Some(0), _) => Verdict::yes_in("TW"),
        (_, Some(0)) => Verdict::Yes {
            region: Some("HK/MO".to_owned()),
        },
        (Some(-10403), _) | (_, Some(-10403)) => Verdict::no(),
        _ if tw.status == 412 || hmt.status == 412 => Verdict::BlockedWaf,
        _ => Verdict::Unexpected(format!("HTTP {}/{}", tw.status, hmt.status)),
    }
}

fn bilibili_code(response: &HttpResponse) -> Option<i64> {
    super::json(response)?
        .get("code")
        .and_then(serde_json::Value::as_i64)
}

/// Extracts `deviceid` from Bahamut's `getdeviceid.php`.
#[must_use]
pub fn bahamut_device_id(response: &HttpResponse) -> Option<String> {
    if super::gate(response).is_some() || response.status != 200 {
        return None;
    }
    let val = super::json(response)?;
    let id = val.get("deviceid")?.as_str()?.trim();
    (!id.is_empty()).then(|| id.to_owned())
}

/// Failure verdict when `getdeviceid.php` returned no device id.
#[must_use]
pub fn bahamut_device_failure(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Parses Bahamut's `token.php` answer.
#[must_use]
pub fn bahamut_token(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let Some(val) = super::json(response) else {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    };
    if val.get("animeSn").is_some() {
        let region = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code)
            .unwrap_or_else(|| "TW".to_owned());
        return Verdict::yes_in(region);
    }
    if val.get("error").is_some() {
        return Verdict::no();
    }
    Verdict::Unexpected("unrecognized Bahamut token response".to_owned())
}

/// Viu (`https://www.viu.com/`).
#[must_use]
pub fn viu(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    let lower_url = response.final_url.to_ascii_lowercase();
    if lower_url.contains("no-service") || matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(seg) = super::extract_between(&lower_url, "/ott/", "/")
        && let Some(code) = super::country_code(seg)
    {
        return Verdict::yes_in(code);
    }
    let body = response.text();
    if body.contains("no-service") || body.contains("not available in your country") {
        return Verdict::no();
    }
    if response.status == 200 {
        let region =
            super::extract_between(&body, "\"countryCode\":\"", "\"").and_then(super::country_code);
        Verdict::Yes { region }
    } else {
        Verdict::Unexpected(format!("HTTP {}", response.status))
    }
}

/// myTV SUPER (`mytvsuper.com/api/auth/getSession/self/`).
#[must_use]
pub fn mytvsuper(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let region = val.get("region").and_then(serde_json::Value::as_i64);
        return match region {
            Some(1) => Verdict::yes_in("HK"),
            Some(_) => Verdict::no(),
            None => Verdict::Unexpected("missing myTV SUPER region".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// HOY TV (`api2.hoy.tv/api/v3/a/channel`).
#[must_use]
pub fn hoy_tv(response: &HttpResponse) -> Verdict {
    super::status_verdict(
        response,
        "HK",
        &["geo", "not available", "outside hong kong", "\"code\":403"],
    )
}

/// KKTV (`api.kktv.me/v3/ipcheck`).
#[must_use]
pub fn kktv(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let data = val.get("data").unwrap_or(&val);
        let allowed = data.get("is_allowed").and_then(serde_json::Value::as_bool);
        let country = data
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        return match (allowed, country.as_deref()) {
            (Some(true), Some(code)) => Verdict::yes_in(code),
            (Some(true), None) => Verdict::yes_in("TW"),
            (None, Some("TW")) => Verdict::yes_in("TW"),
            (Some(false), _) | (_, Some(_)) => Verdict::no(),
            (None, None) => Verdict::Unexpected("unrecognized KKTV payload".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// LiTV (`litv.tv/vod/ajax/getUrl`).
#[must_use]
pub fn litv(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if body.contains("OutsideRegionError") || body.contains("vod.error.outsideregionerror") {
        return Verdict::no();
    }
    if response.status == 200 && (body.contains("fullpath") || body.contains("errorMessage\":null"))
    {
        return Verdict::yes_in("TW");
    }
    if let Some(val) = super::json(response) {
        if val
            .get("errorMessage")
            .is_some_and(serde_json::Value::is_null)
        {
            return Verdict::yes_in("TW");
        }
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// MyVideo (`myvideo.net.tw`).
#[must_use]
pub fn myvideo(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if response.final_url.contains("serviceAreaBlock") || matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    super::status_verdict(response, "TW", &["serviceAreaBlock"])
}

/// LINE TV (`linetv.tw/api/part/...`).
#[must_use]
pub fn linetv(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        if val.get("eps_info").is_some() || val.get("allowCountries").is_some() {
            let allow = val
                .get("allowCountries")
                .and_then(serde_json::Value::as_array);
            if let Some(list) = allow
                && list.is_empty()
            {
                return Verdict::no();
            }
            return Verdict::yes_in("TW");
        }
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// friDay Video (`video.friday.tw`).
#[must_use]
pub fn friday_video(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    // 1003 or "\u{6d77}\u{5916}" ("overseas") / "\u{5730}\u{533a}" ("region") marks geo-block.
    if body.contains("\"code\":1003")
        || body.contains("\u{6d77}\u{5916}")
        || body.contains("\u{5730}\u{533a}")
    {
        return Verdict::no();
    }
    if response.status == 200 && body.contains("\"code\":") {
        return Verdict::yes_in("TW");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// CATCHPLAY+ (`sunapi.catchplay.com/geo`).
#[must_use]
pub fn catchplay(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let code = val
            .get("isoCode")
            .or_else(|| val.get("code"))
            .or_else(|| val.get("country"))
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        return match code.as_deref() {
            Some("TW" | "ID" | "SG") => Verdict::Yes { region: code },
            Some(_) => Verdict::no(),
            None => Verdict::Unexpected("missing CATCHPLAY country".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Hami Video (`hamivideo.hinet.net/api/play.do`).
#[must_use]
pub fn hamivideo(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let code = val.get("code").and_then(serde_json::Value::as_str);
        return match code {
            Some("06001-107" | "06001-000" | "00000") => Verdict::yes_in("TW"),
            Some("06001-106") => Verdict::no(),
            Some(_) if val.get("url").is_some() => Verdict::yes_in("TW"),
            Some(_) => Verdict::no(),
            None => Verdict::Unexpected("missing Hami Video code".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Abema (`api.abema.io/v1/ip/check`).
#[must_use]
pub fn abema(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let country = val
            .get("isoCountryCode")
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        return match country.as_deref() {
            Some("JP") => Verdict::yes_in("JP"),
            Some(_) => Verdict::no(),
            None => Verdict::Unexpected("missing Abema isoCountryCode".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// DMM TV (`api.dmm.com/service/digitalapi/-/ip_check/`).
#[must_use]
pub fn dmm(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    let body = response.text();
    if matches!(response.status, 403 | 451)
        || body.contains("\"is_foreign\":true")
        || body.contains("foreign service")
        || body.contains("not available in your region")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("JP");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Extracts `(platform_uid, platform_token)` from TVer's `browser/create` response.
#[must_use]
pub fn tver_credentials(response: &HttpResponse) -> Option<(String, String)> {
    if super::gate(response).is_some() || response.status != 200 {
        return None;
    }
    let val = super::json(response)?;
    let result = val.get("result").unwrap_or(&val);
    let uid = result.get("platform_uid")?.as_str()?.trim();
    let token = result.get("platform_token")?.as_str()?.trim();
    if uid.is_empty() || token.is_empty() {
        return None;
    }
    Some((uid.to_owned(), token.to_owned()))
}

/// Verdict when TVer's `browser/create` gave no credentials.
#[must_use]
pub fn tver_create_verdict(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// TVer `callHome` availability check.
#[must_use]
pub fn tver_home(response: &HttpResponse) -> Verdict {
    super::status_verdict(response, "JP", &["forbidden", "outside japan"])
}

/// Niconico (`nicovideo.jp/watch/so23017073`).
#[must_use]
pub fn niconico(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if body.contains("same_region")
        || body.contains("notAllowedCountry")
        || body.contains("cannot be played in your country")
    {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("JP");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// U-NEXT (`video-api.unext.jp/api/1/player/url`).
#[must_use]
pub fn u_next(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let status = val
            .pointer("/data/result_status")
            .or_else(|| val.get("code"))
            .and_then(serde_json::Value::as_i64);
        return match status {
            // 467 is U-NEXT's overseas IP restriction code; 470/475/200/0 mean domestic auth check.
            Some(467) => Verdict::no(),
            Some(0 | 200 | 470 | 475) => Verdict::yes_in("JP"),
            _ if response.status == 200 => Verdict::yes_in("JP"),
            _ => Verdict::Unexpected("unrecognized U-NEXT code".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Lemino (`if.lemino.docomo.ne.jp`).
#[must_use]
pub fn lemino(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 400 | 200) {
        return Verdict::yes_in("JP");
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// NHK+ (`plus.nhk.jp/api/v1/ipcheck`).
#[must_use]
pub fn nhk_plus(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        let allowed = val
            .get("is_domestic")
            .or_else(|| val.pointer("/data/is_domestic"))
            .or_else(|| val.get("allowed"))
            .and_then(serde_json::Value::as_bool);
        let country = val
            .get("country_code")
            .and_then(serde_json::Value::as_str)
            .and_then(super::country_code);
        return match (allowed, country.as_deref()) {
            (Some(true), _) | (None, Some("JP")) => Verdict::yes_in("JP"),
            (Some(false), _) | (None, Some(_)) => Verdict::no(),
            (None, None) => Verdict::Unexpected("unrecognized NHK+ payload".to_owned()),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// radiko (`radiko.jp/area`).
#[must_use]
pub fn radiko(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if body.contains("OUT") {
        return Verdict::no();
    }
    if response.status == 200 && body.contains("JP") {
        return Verdict::yes_in("JP");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Wavve (`apis.wavve.com/fz/streaming`).
#[must_use]
pub fn wavve(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    let body = response.text();
    // 551 or "\u{d574}\u{c678}" ("overseas" in Korean) marks geo-restriction.
    if matches!(response.status, 403 | 451)
        || body.contains("\"resultcode\":\"551\"")
        || body.contains("\u{d574}\u{c678}")
        || body.contains("overseas")
    {
        return Verdict::no();
    }
    if response.status == 200 || body.contains("resultcode") {
        return Verdict::yes_in("KR");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// TVING (`api.tving.com/v2a/media/stream/info`).
#[must_use]
pub fn tving(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    // 024 or "\u{d574}\u{c678}" ("overseas" in Korean) marks geo-restriction.
    if body.contains("\"code\":\"024\"")
        || body.contains("\u{d574}\u{c678}")
        || body.contains("not service")
    {
        return Verdict::no();
    }
    if response.status == 200 && body.contains("header") {
        return Verdict::yes_in("KR");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Coupang Play (`coupangplay.com`).
#[must_use]
pub fn coupang_play(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(loc) = response.header("location") {
        if loc.contains("not-available") || loc.contains("unavailable") {
            return Verdict::no();
        }
        return Verdict::yes_in("KR");
    }
    if response.status == 200 {
        return Verdict::yes_in("KR");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// mewatch (`cdn.mewatch.sg`).
#[must_use]
pub fn mewatch(response: &HttpResponse) -> Verdict {
    super::status_verdict(
        response,
        "SG",
        &[
            "geo_blocked",
            "GeoBlocked",
            "not available in your location",
        ],
    )
}
