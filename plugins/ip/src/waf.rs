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

/// Detects Cloudflare Turnstile, Cloudflare JS challenge, AWS WAF, DataDome, PerimeterX,
/// Imperva, and Akamai anti-bot challenge responses across `ip.quality` and `ip.media`.
#[must_use]
pub(crate) fn is_challenge(response: &HttpResponse) -> bool {
    if response
        .header("cf-mitigated")
        .is_some_and(|val| val.trim().eq_ignore_ascii_case("challenge"))
    {
        return true;
    }
    if response.header("x-amzn-waf-action").is_some_and(|val| {
        let trimmed = val.trim();
        trimmed.eq_ignore_ascii_case("challenge") || trimmed.eq_ignore_ascii_case("captcha")
    }) {
        return true;
    }
    if response
        .header("x-dd-b")
        .is_some_and(|val| !val.trim().is_empty())
    {
        return true;
    }
    if response.status == 200
        && contains_ascii_ci(&response.body, b"_incapsula_resource")
        && (contains_ascii_ci(&response.body, b"main-iframe")
            || contains_ascii_ci(&response.body, b"incapsula incident id"))
    {
        return true;
    }
    if !matches!(response.status, 403 | 503) {
        return false;
    }
    if response
        .header("x-datadome")
        .is_some_and(|val| !val.trim().is_empty())
    {
        return true;
    }
    CHALLENGE_MARKERS
        .iter()
        .any(|marker| contains_ascii_ci(&response.body, marker.as_bytes()))
}

fn contains_ascii_ci(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack
            .windows(needle.len())
            .any(|win| win.eq_ignore_ascii_case(needle))
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
    fn detects_case_insensitive_cloudflare_aws_datadome_imperva_and_akamai_challenges() {
        for marker in super::CHALLENGE_MARKERS {
            assert!(
                super::is_challenge(&resp(403, &[], &marker.to_ascii_uppercase())),
                "403 marker {marker}"
            );
            assert!(
                super::is_challenge(&resp(503, &[], marker)),
                "503 marker {marker}"
            );
        }
        assert!(super::is_challenge(&resp(
            200,
            &[("cf-mitigated", " Challenge ")],
            ""
        )));
        assert!(super::is_challenge(&resp(
            202,
            &[("x-amzn-waf-action", "challenge")],
            ""
        )));
        assert!(super::is_challenge(&resp(
            405,
            &[("x-amzn-waf-action", "CAPTCHA")],
            ""
        )));
        assert!(super::is_challenge(&resp(
            401,
            &[("x-dd-b", "1")],
            "blocked"
        )));
        assert!(!super::is_challenge(&resp(403, &[("x-dd-b", "  ")], "ok")));
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
            200,
            &[],
            "<iframe id=\"main-iframe\" src=\"/_Incapsula_Resource?SWJIYLWA=719d\"></iframe>"
        )));
        assert!(!super::is_challenge(&resp(
            200,
            &[],
            "<script src=\"/_Incapsula_Resource?b=1\"></script><html>ok</html>"
        )));
        assert!(!super::is_challenge(&resp(
            403,
            &[],
            r#"{"errors":["GeoBlocked"]}"#
        )));
    }
}
