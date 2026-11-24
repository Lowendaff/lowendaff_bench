//! Bilibili unlock probe: compares Taiwan-only and HK/MO/TW PGC bangumi `playurl` responses.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Bilibili.
pub const SPEC: Platform = Platform {
    id: "bilibili",
    name: "Bilibili",
    probe,
};

/// Probes Bilibili's Taiwan-only and HK/MO/TW bangumi endpoints concurrently over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let tw_req = HttpRequest::get(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "tw",
                "https://api.bilibili.com/pgc/player/web/playurl?ep_id=507946&qn=80&fnval=16",
            ),
        );
        let hmt_req = HttpRequest::get(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "hmt",
                "https://api.bilibili.com/pgc/player/web/playurl?ep_id=285145&qn=80&fnval=16",
            ),
        );
        let (tw, hmt) = tokio::join!(ctx.probe(&tw_req), ctx.probe(&hmt_req));
        match (tw, hmt) {
            (Ok(tw_res), Ok(hmt_res)) => parse(&tw_res, &hmt_res),
            (Err(err), _) | (_, Err(err)) => parse::error_verdict(&err),
        }
    })
}

/// Evaluates Bilibili's Taiwan-only (`tw`) and HK/MO/TW (`hmt`) responses.
#[must_use]
pub fn parse(tw: &HttpResponse, hmt: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(tw) {
        return early;
    }
    if let Some(early) = parse::gate(hmt) {
        return early;
    }
    let tw_code = code(tw);
    let hmt_code = code(hmt);
    if tw.status == 412
        || hmt.status == 412
        || matches!(tw_code, Some(-412))
        || matches!(hmt_code, Some(-412))
    {
        return Verdict::BlockedWaf;
    }
    match (tw_code, hmt_code) {
        (Some(0), _) => Verdict::yes_in("TW"),
        (Some(-10403), Some(0)) => Verdict::Yes {
            region: Some("HK/MO".to_owned()),
        },
        (Some(-10403), Some(-10403)) => Verdict::no(),
        _ => Verdict::Unexpected(format!("HTTP {}/{}", tw.status, hmt.status)),
    }
}

fn code(response: &HttpResponse) -> Option<i64> {
    parse::json(response)?
        .get("code")
        .and_then(serde_json::Value::as_i64)
}
