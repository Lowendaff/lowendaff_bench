//! Hulu US storefront and authentication geo-block probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Restriction;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Hulu.
pub const SPEC: Platform = Platform {
    id: "hulu",
    name: "Hulu",
    probe,
};

/// Probes Hulu over `family`: checks storefront redirect first (overseas redirects to Disney+ or
/// `hulu.jp`), then verifies US authentication geo-check when `www.hulu.com` serves 200.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let web_req = HttpRequest::get(
            family,
            parse::endpoint(base_url, SPEC.id, "check", "https://www.hulu.com/"),
        );
        let web_res = match parse::send(ctx, web_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        if web_res.status != 200 || base_url.is_some() {
            return parse(&web_res);
        }
        let pre = parse(&web_res);
        if matches!(
            pre,
            Verdict::No { .. } | Verdict::BlockedWaf | Verdict::RateLimited
        ) {
            return pre;
        }
        let auth_req = HttpRequest::post(
            family,
            "https://auth.hulu.com/v4/web/password/authenticate".to_owned(),
            b"user_email=&password=&scenario=web_password_login".to_vec(),
        )
        .with_header("accept", "application/json")
        .with_header(
            "content-type",
            "application/x-www-form-urlencoded; charset=utf-8",
        )
        .with_header("origin", "https://www.hulu.com")
        .with_header("referer", "https://www.hulu.com/welcome");
        match parse::send(ctx, auth_req).await {
            Ok(auth_res) => parse(&auth_res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses Hulu's storefront or authentication HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if (300..400).contains(&response.status)
        && let Some(loc) = response.header("location")
    {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("unavailable") || lower.contains("geo") || lower.contains("disneyplus") {
            return Verdict::no();
        }
        if lower.contains("hulu.jp") {
            return Verdict::yes_in("JP");
        }
    }
    let body = response.text();
    let lower = body.to_ascii_lowercase();
    if lower.contains("anonymous proxy tool") || lower.contains("unblocker") {
        return Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        };
    }
    if matches!(response.status, 403 | 406 | 451)
        || lower.contains("geo_blocked")
        || lower.contains("not available in your region")
    {
        return Verdict::no();
    }
    if lower.contains("login_forbidden")
        || lower.contains("login_bad_request")
        || lower.contains("your login is invalid")
    {
        return Verdict::yes_in("US");
    }
    if response.status == 200 && lower.contains("hulu") {
        return Verdict::yes_in("US");
    }
    if (200..400).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
