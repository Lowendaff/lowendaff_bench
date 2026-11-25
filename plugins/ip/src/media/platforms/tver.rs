//! TVer unlock probe: discovers a live catch-up episode via `callHome` and checks Brightcove
//! playback geolocation (`CLIENT_GEO`).

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

/// Probes TVer dynamically over `family`.
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

        let home_url = match base_url {
            Some(base) => format!("{base}/tver/callapi"),
            None => format!(
                "https://platform-api.tver.jp/service/api/v1/callHome\
                 ?platform_uid={uid}&platform_token={token}&require_data=mylist%2Cresume%2Clater"
            ),
        };
        let home_req = HttpRequest::get(family, home_url)
            .with_header("origin", "https://tver.jp")
            .with_header("referer", "https://tver.jp/")
            .with_header("x-tver-platform-type", "web")
            .with_max_bytes(2 * 1024 * 1024);
        let home_res = match parse::send(ctx, home_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        if base_url.is_some() {
            return parse(&home_res);
        }
        let Some(ep_id) = episode_id(&home_res) else {
            return parse(&home_res);
        };

        let ep_req = HttpRequest::get(
            family,
            format!("https://statics.tver.jp/content/episode/{ep_id}.json"),
        )
        .with_header("origin", "https://tver.jp")
        .with_header("referer", "https://tver.jp/");
        let ep_res = match parse::send(ctx, ep_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        let Some((account_id, player_id, video_ref)) = episode_video(&ep_res) else {
            return Verdict::Unexpected("missing TVer episode video metadata".to_owned());
        };

        let player_req = HttpRequest::get(
            family,
            format!("https://players.brightcove.net/{account_id}/{player_id}_default/index.min.js"),
        )
        .with_max_bytes(1024 * 1024);
        let player_res = match parse::send(ctx, player_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        let Some(policy_key) = parse::extract_between(&player_res.text(), "policyKey:\"", "\"")
            .map(str::trim)
            .filter(|pk| !pk.is_empty())
            .map(str::to_owned)
        else {
            return Verdict::Unexpected("missing TVer Brightcove policyKey".to_owned());
        };

        let edge_url = format!(
            "https://edge.api.brightcove.com/playback/v1/accounts/{account_id}/videos/{video_ref}"
        );
        let edge_req = HttpRequest::get(family, edge_url)
            .with_header("accept", format!("application/json;pk={policy_key}"))
            .with_header("origin", "https://tver.jp")
            .with_header("referer", "https://tver.jp/");
        match parse::send(ctx, edge_req).await {
            Ok(edge_res) => parse(&edge_res),
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

fn episode_id(response: &HttpResponse) -> Option<String> {
    let val = parse::json(response)?;
    let components = val.pointer("/result/components")?.as_array()?;
    for comp in components {
        let Some(contents) = comp.get("contents").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for item in contents {
            let candidate = item
                .pointer("/content/id")
                .or_else(|| item.get("id"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim);
            if let Some(id) = candidate
                && id.starts_with("ep")
                && (8..=16).contains(&id.len())
                && id.bytes().all(|b| b.is_ascii_alphanumeric())
            {
                return Some(id.to_owned());
            }
        }
    }
    None
}

fn episode_video(response: &HttpResponse) -> Option<(String, String, String)> {
    let val = parse::json(response)?;
    let video = val.get("video")?;
    let safe = |raw: &str| {
        let trimmed = raw.trim();
        (!trimmed.is_empty()
            && trimmed
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')))
        .then(|| trimmed.to_owned())
    };
    let account = safe(video.get("accountID")?.as_str()?)?;
    let player = safe(video.get("playerID")?.as_str()?)?;
    let target = if let Some(vid) = video
        .get("videoID")
        .and_then(serde_json::Value::as_str)
        .and_then(safe)
    {
        vid
    } else {
        let ref_id = safe(video.get("videoRefID")?.as_str()?)?;
        format!("ref:{ref_id}")
    };
    Some((account, player, target))
}

/// Evaluates TVer's Brightcove playback or `callHome` response.
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    let body = response.text();
    let lower = body.to_ascii_lowercase();
    if lower.contains("client_geo") || lower.contains("outside japan") {
        return Verdict::no();
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        if val.get("account_id").is_some() && response.status == 200 {
            return Verdict::yes_in("JP");
        }
        let err_geo = [
            val.get("message"),
            val.get("error"),
            val.pointer("/result/message"),
        ]
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .any(|msg| {
            let m = msg.to_ascii_lowercase();
            m.contains("forbidden") || m.contains("outside japan") || m.contains("client_geo")
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
    if lower.contains("forbidden") {
        return Verdict::no();
    }
    if (200..300).contains(&response.status) {
        return parse::no_region();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
