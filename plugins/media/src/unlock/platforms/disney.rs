//! Disney+ unlock probe: checks storefront redirect headers and BAMGrid session location metadata.

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

/// Platform specification for Disney+.
pub const SPEC: Platform = Platform {
    id: "disney",
    name: "Disney+",
    probe,
};

/// Probes Disney+'s web root and BAMGrid public GraphQL endpoint over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let web_req = HttpRequest::get(
            family,
            parse::endpoint(base_url, SPEC.id, "web", "https://www.disneyplus.com/"),
        );
        let web = match parse::send(ctx, web_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        if let Some(early) = web_early(&web) {
            return early;
        }
        let gql_payload = br#"{"query":"query { __typename }"}"#.to_vec();
        let api_req = HttpRequest::post(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "api",
                "https://disney.api.edge.bamgrid.com/v1/public/graphql",
            ),
            gql_payload,
        )
        .with_header("content-type", "application/json");
        let api = ctx.probe(&api_req).await.ok();
        parse(&web, api.as_ref())
    })
}

/// Checks Disney+'s initial web response for immediate geo-block redirects.
#[must_use]
pub fn web_early(web: &HttpResponse) -> Option<Verdict> {
    if let Some(early) = parse::gate(web) {
        return Some(early);
    }
    if let Some(loc) = web.header("location") {
        let lower = loc.to_ascii_lowercase();
        if lower.contains("unavailable") || lower.contains("unsupported") {
            return Some(Verdict::no());
        }
        if lower.contains("preview") || lower.contains("coming-soon") {
            return Some(Verdict::No {
                restriction: Some(Restriction::ComingSoon),
            });
        }
    }
    if web.status == 403 || web.status == 451 {
        return Some(Verdict::no());
    }
    None
}

/// Evaluates Disney+'s web and optional BAMGrid GraphQL responses.
#[must_use]
pub fn parse(web: &HttpResponse, api: Option<&HttpResponse>) -> Verdict {
    if let Some(early) = web_early(web) {
        return early;
    }
    if let Some(api_res) = api
        && let Some(val) = parse::json(api_res)
    {
        let session = val
            .pointer("/extensions/sdk/session")
            .or_else(|| val.pointer("/data/session"));
        if let Some(session) = session {
            let supported = session
                .pointer("/location/inSupportedLocation")
                .or_else(|| session.get("inSupportedLocation"))
                .and_then(serde_json::Value::as_bool);
            let country = session
                .pointer("/location/countryCode")
                .or_else(|| session.get("countryCode"))
                .and_then(serde_json::Value::as_str)
                .and_then(parse::country_code);
            if supported == Some(false) {
                return Verdict::no();
            }
            if supported == Some(true) || country.is_some() {
                return Verdict::Yes { region: country };
            }
        }
    }
    let body = web.text();
    if body.contains("Disney+ is not available in your region")
        || body.contains("unavailable in your location")
    {
        return Verdict::no();
    }
    let region = web
        .header("location")
        .and_then(parse::url_country)
        .or_else(|| {
            parse::extract_between(&body, "\"countryCode\":\"", "\"").and_then(parse::country_code)
        });
    if matches!(web.status, 200 | 301 | 302 | 307 | 308) {
        Verdict::Yes { region }
    } else {
        Verdict::Unexpected(format!("HTTP {}", web.status))
    }
}
