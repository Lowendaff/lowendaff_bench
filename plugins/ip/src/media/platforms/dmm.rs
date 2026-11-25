//! DMM TV Japan GraphQL `isForeignAccess` probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for DMM TV.
pub const SPEC: Platform = Platform {
    id: "dmm",
    name: "DMM TV",
    probe,
};

/// Probes DMM TV over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::post(
            family,
            url("https://api.tv.dmm.com/graphql"),
            br#"{"query":"query FetchClient { client { isForeignAccess } }"}"#.to_vec(),
        )
        .with_header("content-type", "application/json");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses DMM TV's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let lower = response.text().to_ascii_lowercase();
    if lower.contains("foreign service") || lower.contains("not available in your region") {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let is_foreign = [
            "/data/client/isForeignAccess",
            "/is_foreign",
            "/result/is_foreign",
            "/data/is_foreign",
        ]
        .into_iter()
        .find_map(|ptr| val.pointer(ptr).and_then(serde_json::Value::as_bool));
        let country = ["/country_code", "/result/country_code"]
            .into_iter()
            .find_map(|ptr| {
                val.pointer(ptr)
                    .and_then(serde_json::Value::as_str)
                    .and_then(parse::country_code)
            });
        match (is_foreign, country.as_deref()) {
            (Some(true), _) => return Verdict::no(),
            (_, Some(cc)) if cc != "JP" => return Verdict::no(),
            (Some(false), _) | (None, Some("JP")) if response.status == 200 => {
                return Verdict::yes_in("JP");
            }
            _ => {}
        }
        return Verdict::Unexpected("unrecognized DMM payload".to_owned());
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
