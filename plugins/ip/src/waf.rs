//! Shared anti-bot / WAF challenge-page detector for `ip.quality` and `ip.media`.

use bench_api::HttpResponse;

const CHALLENGE_MARKERS: [&str; 9] = [
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

/// Detects Cloudflare Turnstile, Cloudflare JS challenge, DataDome, PerimeterX, Imperva, and
/// Akamai anti-bot challenge responses across `ip.quality` and `ip.media`.
#[must_use]
pub(crate) fn is_challenge(response: &HttpResponse) -> bool {
    if response
        .header("cf-mitigated")
        .is_some_and(|val| val.eq_ignore_ascii_case("challenge"))
    {
        return true;
    }
    if !matches!(response.status, 403 | 503) {
        return false;
    }
    if response.header("x-datadome").is_some() || response.header("x-dd-b").is_some() {
        return true;
    }
    let body = response.text().to_ascii_lowercase();
    CHALLENGE_MARKERS.iter().any(|marker| body.contains(marker))
}

#[cfg(test)]
mod tests {
    use bench_api::HttpResponse;

    fn resp(status: u16, headers: &[(&str, &str)], body: &str) -> HttpResponse {
        HttpResponse {
            status,
            final_url: "https://example.com/".to_owned(),
            headers: headers
                .iter()
                .map(|&(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn detects_case_insensitive_cloudflare_datadome_and_akamai_challenges() {
        assert!(super::is_challenge(&resp(
            403,
            &[],
            "<HTML><HEAD><TITLE>Just a moment...</TITLE></HEAD></HTML>"
        )));
        assert!(super::is_challenge(&resp(
            403,
            &[("x-dd-b", "1")],
            "blocked"
        )));
        assert!(super::is_challenge(&resp(
            403,
            &[("X-DataDome", "protected")],
            "blocked"
        )));
        assert!(!super::is_challenge(&resp(
            200,
            &[("x-datadome", "protected")],
            "ok"
        )));
        assert!(super::is_challenge(&resp(
            403,
            &[],
            "Reference #18.abc https://errors.edgesuite.net/18.abc"
        )));
        assert!(!super::is_challenge(&resp(
            403,
            &[],
            r#"{"errors":["GeoBlocked"]}"#
        )));
    }
}
