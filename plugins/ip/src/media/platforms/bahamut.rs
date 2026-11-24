//! Bahamut Anime Crazy unlock probe: two-step `getdeviceid.php` + `token.php` check.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

/// Platform specification for Bahamut Anime.
pub const SPEC: Platform = Platform {
    id: "bahamut",
    name: "Bahamut Anime",
    probe,
};

/// Probes Bahamut Anime Crazy in two steps over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let dev_req = HttpRequest::get(
            family,
            parse::endpoint(
                base_url,
                SPEC.id,
                "device",
                "https://ani.gamer.com.tw/ajax/getdeviceid.php",
            ),
        );
        let dev_res = match parse::send(ctx, dev_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        let Some(dev_id) = device_id(&dev_res) else {
            return device_failure(&dev_res);
        };
        let token_url = match base_url {
            Some(base) => format!("{base}/bahamut/token?device={dev_id}"),
            None => format!(
                "https://ani.gamer.com.tw/ajax/token.php?adID=89422&sn=37783&device={dev_id}"
            ),
        };
        let tok_req = HttpRequest::get(family, token_url).with_header(
            "referer",
            "https://ani.gamer.com.tw/animeVideo.php?sn=37783",
        );
        match parse::send(ctx, tok_req).await {
            Ok(tok_res) => parse(&tok_res),
            Err(verdict) => verdict,
        }
    })
}

/// Extracts `deviceid` from `getdeviceid.php`.
#[must_use]
pub fn device_id(response: &HttpResponse) -> Option<String> {
    if parse::gate(response).is_some() || response.status != 200 {
        return None;
    }
    let val = parse::json(response)?;
    let id = val.get("deviceid")?.as_str()?.trim();
    let safe = (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
    safe.then(|| id.to_owned())
}

/// Maps a failed `getdeviceid.php` response to a [`Verdict`].
#[must_use]
pub fn device_failure(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

/// Parses `token.php`'s JSON payload.
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if response.status != 200 {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    }
    let Some(val) = parse::json(response) else {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    };
    let has_sn = val.get("animeSn").is_some_and(|v| {
        v.as_i64().is_some_and(|n| n > 0)
            || v.as_str()
                .and_then(|s| s.trim().parse::<i64>().ok())
                .is_some_and(|n| n > 0)
    });
    if has_sn {
        let region = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code)
            .unwrap_or_else(|| "TW".to_owned());
        return Verdict::yes_in(region);
    }
    if let Some(err_val) = val
        .get("error")
        .filter(|v| !v.is_null() && v.as_bool() != Some(false))
    {
        let code = err_val.get("code").and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        });
        let msg = err_val
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let is_geo = code == Some(1007)
            || msg.contains("region")
            || msg.contains("area")
            || msg.contains("country")
            || msg.contains("\u{5730}\u{5340}")
            || msg.contains("\u{5340}\u{57df}");
        if let Some(code) = code
            && !is_geo
        {
            return Verdict::Unexpected(format!("Bahamut error code {code}"));
        }
        return Verdict::no();
    }
    Verdict::Unexpected("unrecognized Bahamut token response".to_owned())
}
