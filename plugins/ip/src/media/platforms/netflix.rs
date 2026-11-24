//! Netflix unlock probe: compares non-original (`/title/81280792`) and original (`/title/80018499`)
//! title responses.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Restriction;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Netflix.
pub const SPEC: Platform = Platform {
    id: "netflix",
    name: "Netflix",
    probe,
};

/// Probes Netflix's licensed and original titles concurrently over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let follow = RedirectPolicy::Follow { max: 5 };
        let licensed_req = HttpRequest::get(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "licensed",
                "https://www.netflix.com/title/81280792",
            ),
        )
        .with_redirect(follow);
        let original_req = HttpRequest::get(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "original",
                "https://www.netflix.com/title/80018499",
            ),
        )
        .with_redirect(follow);
        let (licensed, original) = tokio::join!(ctx.probe(&licensed_req), ctx.probe(&original_req));
        match (licensed, original) {
            (Ok(lic), Ok(orig)) => parse(&lic, &orig),
            (Err(err), _) | (_, Err(err)) => parse::error_verdict(&err),
        }
    })
}

/// Evaluates Netflix's licensed and original title responses.
#[must_use]
pub fn parse(licensed: &HttpResponse, original: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(licensed) {
        return early;
    }
    if let Some(early) = parse::gate(original) {
        return early;
    }
    let lic_state = page_state(licensed);
    let orig_state = page_state(original);
    let region = extract_region(licensed).or_else(|| extract_region(original));
    match (lic_state, orig_state) {
        (TitleState::Proxy, _) | (_, TitleState::Proxy) => Verdict::No {
            restriction: Some(Restriction::ProxyDetected),
        },
        (TitleState::Available, _) => Verdict::Yes { region },
        (TitleState::Blocked, TitleState::Available) => Verdict::OriginalsOnly { region },
        (TitleState::Blocked, TitleState::Blocked) => Verdict::no(),
        (TitleState::Unknown(status), _) | (_, TitleState::Unknown(status)) => {
            Verdict::Unexpected(format!("HTTP {status}"))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TitleState {
    Available,
    Blocked,
    Proxy,
    Unknown(u16),
}

fn page_state(response: &HttpResponse) -> TitleState {
    let body = response.text();
    if body.contains("NSES-UH4") || body.contains("unblocker or proxy") {
        return TitleState::Proxy;
    }
    if matches!(response.status, 403 | 404 | 451) {
        return TitleState::Blocked;
    }
    if body.contains("Not Available")
        || body.contains("page-404")
        || response.final_url.contains("NotAvailable")
    {
        return TitleState::Blocked;
    }
    if (200..300).contains(&response.status) {
        TitleState::Available
    } else {
        TitleState::Unknown(response.status)
    }
}

fn extract_region(response: &HttpResponse) -> Option<String> {
    if let Some(code) = parse::url_country(&response.final_url) {
        return Some(code);
    }
    let body = response.text();
    for (start, end) in [
        ("\"requestCountry\":{\"id\":\"", "\""),
        ("\\\"requestCountry\\\":{\\\"id\\\":\\\"", "\\\""),
        ("\\x22requestCountry\\x22:{\\x22id\\x22:\\x22", "\\x22"),
        ("\"country\":\"", "\""),
        ("\\\"country\\\":\\\"", "\\\""),
        ("\\x22country\\x22:\\x22", "\\x22"),
    ] {
        if let Some(code) = parse::extract_between(&body, start, end).and_then(parse::country_code)
        {
            return Some(code);
        }
    }
    None
}
