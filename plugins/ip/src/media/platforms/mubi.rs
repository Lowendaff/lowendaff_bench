//! MUBI `/v3/country` availability probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for MUBI.
pub const SPEC: Platform = Platform {
    id: "mubi",
    name: "MUBI",
    probe,
};

/// Probes MUBI over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://api.mubi.com/v3/country"))
            .with_header("client", "web");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses MUBI's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let available = val.get("available").and_then(serde_json::Value::as_bool);
        if available == Some(false) {
            return Verdict::no();
        }
        let field_code = |key: &str| {
            val.get(key)
                .and_then(serde_json::Value::as_str)
                .and_then(parse::country_code)
        };
        let code = field_code("code").or_else(|| field_code("country"));
        if matches!(code.as_deref(), Some("CN" | "KP" | "IR" | "SY" | "CU")) {
            return Verdict::no();
        }
        if (200..300).contains(&response.status) && (code.is_some() || available == Some(true)) {
            return Verdict::Yes { region: code };
        }
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
