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
    (!id.is_empty()).then(|| id.to_owned())
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
    let Some(val) = parse::json(response) else {
        return Verdict::Unexpected(format!("HTTP {}", response.status));
    };
    if val.get("animeSn").is_some() {
        let region = val
            .get("country")
            .and_then(serde_json::Value::as_str)
            .and_then(parse::country_code)
            .unwrap_or_else(|| "TW".to_owned());
        return Verdict::yes_in(region);
    }
    if val.get("error").is_some() {
        return Verdict::no();
    }
    Verdict::Unexpected("unrecognized Bahamut token response".to_owned())
}
