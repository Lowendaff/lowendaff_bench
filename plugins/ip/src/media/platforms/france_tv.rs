//! france.tv EdgeScape country probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for france.tv.
pub const SPEC: Platform = Platform {
    id: "france-tv",
    name: "france.tv",
    probe,
};

/// Probes france.tv over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::get(family, url("https://geo-info.ftven.fr/ws/edgescape.json"));
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses france.tv's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let code_blocked = val
            .get("code")
            .is_some_and(|code| code.as_i64() == Some(2003) || code.as_str() == Some("2003"));
        if code_blocked {
            return Verdict::no();
        }
        let cc = [
            "/reponse/geo_info/country_code",
            "/geo_info/country_code",
            "/country_code",
        ]
        .into_iter()
        .find_map(|ptr| {
            val.pointer(ptr)
                .and_then(serde_json::Value::as_str)
                .and_then(parse::country_code)
        });
        if let Some(code) = cc {
            return if matches!(code.as_str(), "FR" | "GF" | "GP" | "MQ" | "RE" | "YT")
                && (200..300).contains(&response.status)
            {
                Verdict::yes_in(code)
            } else {
                Verdict::no()
            };
        }
    }
    let body = response.text().to_lowercase();
    if body.contains("geoblocage")
        || body.contains("g\u{00e9}oblocage")
        || body.contains("\"code\":2003")
    {
        return Verdict::no();
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
