//! CATCHPLAY+ `sunapi` country probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for CATCHPLAY+.
pub const SPEC: Platform = Platform {
    id: "catchplay",
    name: "CATCHPLAY+",
    probe,
};

/// Probes CATCHPLAY+ over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://sunapi.catchplay.com/geo")).with_header(
            "authorization",
            concat!(
                "Basic NTQ3MzM0NDgtYTU3Yi00MjU2LWE4MTEt",
                "MzdlYzNkNjJmM2E0Ok90QzR3elJRR2hLQ01sSDc2VEoy"
            ),
        );
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses CATCHPLAY+'s HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let err_code = val.get("code").and_then(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        });
        let msg = val
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if err_code.as_deref() == Some("100016") || msg.contains("not allowed") {
            return Verdict::no();
        }
        if (200..300).contains(&response.status) {
            let code = [
                "/data/isoCode",
                "/data/country",
                "/isoCode",
                "/country",
                "/code",
            ]
            .into_iter()
            .find_map(|ptr| {
                val.pointer(ptr)
                    .and_then(serde_json::Value::as_str)
                    .and_then(parse::country_code)
            });
            return match code.as_deref() {
                Some("TW" | "ID" | "SG") => Verdict::Yes { region: code },
                Some(_) => Verdict::no(),
                None => Verdict::Unexpected("missing CATCHPLAY country".to_owned()),
            };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
