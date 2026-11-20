//! LINE TV Taiwan episode metadata probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for LINE TV.
pub const SPEC: Platform = Platform {
    id: "linetv",
    name: "LINE TV",
    probe,
};

/// Probes LINE TV over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(
            family,
            url("https://www.linetv.tw/api/part/11829/eps/1/part?chocomemberId="),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses LINE TV's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
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
