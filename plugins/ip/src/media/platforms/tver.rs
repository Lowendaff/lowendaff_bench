//! TVer unlock probe: creates an anonymous browser token and checks `callHome` availability.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for TVer.
pub const SPEC: Platform = Platform {
    id: "tver",
    name: "TVer",
    probe,
};

/// Probes TVer in two steps over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let create_req = HttpRequest::post(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "create",
                "https://platform-api.tver.jp/v2/api/platform_users/browser/create",
            ),
            b"device_type=pc".to_vec(),
        )
        .with_header("content-type", "application/x-www-form-urlencoded")
        .with_header("origin", "https://s.tver.jp")
        .with_header("referer", "https://s.tver.jp/");
        let create_res = match parse::send(ctx, create_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        let Some((uid, token)) = credentials(&create_res) else {
            return create_failure(&create_res);
        };
        let home = "https://platform-api.tver.jp/service/api/v1/callHome";
        let check_url = match base_url {
            Some(base) => format!("{base}/tver/callapi"),
            None => format!("{home}?platform_uid={uid}&platform_token={token}&require_data=mylist"),
        };
        let check_req = HttpRequest::get(family, check_url)
            .with_header("x-tver-platform-type", "web")
            .with_max_bytes(2 * 1024 * 1024);
        match parse::send(ctx, check_req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Extracts `(platform_uid, platform_token)` from `browser/create`.
#[must_use]
pub fn credentials(response: &HttpResponse) -> Option<(String, String)> {
    if parse::gate(response).is_some() || response.status != 200 {
        return None;
    }
    let val = parse::json(response)?;
    let result = val.get("result").unwrap_or(&val);
    let clean = |raw: &str| {
        let trimmed = raw.trim();
        let valid = !trimmed.is_empty()
            && trimmed.bytes().all(|b| {
                b.is_ascii_graphic()
                    && !matches!(b, b'&' | b'#' | b'?' | b'=' | b'%' | b'+' | b'/' | b'\\')
            });
        valid.then(|| trimmed.to_owned())
    };
    let uid = clean(result.get("platform_uid")?.as_str()?)?;
    let token = clean(result.get("platform_token")?.as_str()?)?;
    Some((uid, token))
}

/// Verdict when `browser/create` gave no credentials.
#[must_use]
pub fn create_failure(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Evaluates TVer's `callHome` response.
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let err_geo = [
            val.get("message"),
            val.get("error"),
            val.pointer("/result/message"),
        ]
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .any(|msg| {
            let lower = msg.to_ascii_lowercase();
            lower.contains("forbidden") || lower.contains("outside japan")
        });
        if err_geo {
            return Verdict::no();
        }
        let code_ok = val.get("code").is_none_or(|code| {
            matches!(code.as_i64(), Some(0 | 200)) || matches!(code.as_str(), Some("0" | "200"))
        });
        let populated = match val.get("result") {
            Some(serde_json::Value::Object(map)) => !map.is_empty(),
            Some(serde_json::Value::Array(items)) => !items.is_empty(),
            _ => false,
        };
        if response.status == 200 && code_ok && populated {
            return Verdict::yes_in("JP");
        }
    }
    let body = response.text().to_ascii_lowercase();
    if body.contains("forbidden") || body.contains("outside japan") {
        return Verdict::no();
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
