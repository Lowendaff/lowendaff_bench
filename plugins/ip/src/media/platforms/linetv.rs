//! LINE TV Taiwan `/api/geo` probe.

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
        let req = HttpRequest::get(family, url("https://www.linetv.tw/api/geo"));
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
        if let Some(code) = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code)
        {
            return if code == "TW" && (200..300).contains(&response.status) {
                Verdict::yes_in("TW")
            } else {
                Verdict::no()
            };
        }
        if val
            .get("inSupportedLocation")
            .and_then(serde_json::Value::as_bool)
            == Some(false)
        {
            return Verdict::no();
        }
        if let Some(list) = val
            .get("allowCountries")
            .and_then(serde_json::Value::as_array)
            && list.is_empty()
        {
            return Verdict::no();
        }
        if let Some(cc) = val.get("countryCode").filter(|v| !v.is_null()) {
            let is_tw = cc.as_i64() == Some(228)
                || cc
                    .as_str()
                    .is_some_and(|s| s == "228" || s.eq_ignore_ascii_case("TW"));
            if !is_tw {
                return Verdict::no();
            }
            if response.status == 200 {
                return Verdict::yes_in("TW");
            }
        }
        if response.status == 200 && val.get("eps_info").is_some_and(|v| !v.is_null()) {
            return Verdict::yes_in("TW");
        }
        return Verdict::Unexpected("unrecognized LINE TV payload".to_owned());
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
