//! Response parsers for the 8 Australian and New Zealand streaming platforms.

use bench_api::HttpResponse;

use super::super::result::Verdict;

/// Stan (`api.stan.com.au/login/v1/sessions/web/account`).
#[must_use]
pub fn stan(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    let body = response.text();
    if body.contains("StreamCo.Concurrency.GeoBlocked")
        || body.contains("AccessDenied")
        || response.status == 403
        || response.status == 451
    {
        return Verdict::no();
    }
    // Without credentials, Australian IPs receive 400/422 validation errors, not GeoBlocked.
    if matches!(response.status, 200 | 400 | 401 | 422) {
        return Verdict::yes_in("AU");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Binge (`auth.streamotion.com.au/oauth/token`).
#[must_use]
pub fn binge(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    match response.status {
        200 | 400 | 401 => Verdict::yes_in("AU"),
        403 | 451 => Verdict::no(),
        status => Verdict::Unexpected(format!("HTTP {status}")),
    }
}

/// Kayo Sports (`api.kayosports.com.au/v3/content/types/landing/names/home`).
#[must_use]
pub fn kayo_sports(response: &HttpResponse) -> Verdict {
    super::status_verdict(
        response,
        "AU",
        &["geoblocked", "not available in your region"],
    )
}

/// 9Now (`tv-api.9now.com.au/v2/pages/livestreams`).
#[must_use]
pub fn nine_now(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if body.contains("\"geoblocked\":true") || body.contains("GeoBlocked") {
        return Verdict::no();
    }
    if response.status == 200 {
        return Verdict::yes_in("AU");
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// 7plus (`7plus.com.au/auth/token`).
#[must_use]
pub fn seven_plus(response: &HttpResponse) -> Verdict {
    if let Some(early) = super::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = super::json(response) {
        if val.get("token").is_some() {
            return Verdict::yes_in("AU");
        }
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// ABC iview (`api.iview.abc.net.au/v2/navigation/mobile`).
#[must_use]
pub fn abc_iview(response: &HttpResponse) -> Verdict {
    super::status_verdict(response, "AU", &["outside australia", "geoblocked"])
}

/// TVNZ+ (`apis-public-prod.tech.tvnz.co.nz/api/v1/web/play/page/categories/live`).
#[must_use]
pub fn tvnz_plus(response: &HttpResponse) -> Verdict {
    super::status_verdict(response, "NZ", &["geoblocked", "outside new zealand"])
}

/// ThreeNow (`now-api.threenow.co.nz/v3/config/web`).
#[must_use]
pub fn threenow(response: &HttpResponse) -> Verdict {
    super::status_verdict(response, "NZ", &["geoblocked", "outside new zealand"])
}
