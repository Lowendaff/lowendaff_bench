//! U-NEXT Japan GraphQL `cosmo_getPlaylistUrl` geo-status probe.

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

const PLAYLIST_QUERY: &str = concat!(
    r#"{"operationName":"cosmo_getPlaylistUrl","#,
    r#""variables":{"code":"ED00479780","playMode":"caption","#,
    r#""bitrateLow":192,"bitrateHigh":null,"validationOnly":false},"#,
    r#""query":"query cosmo_getPlaylistUrl($code: String, $playMode: String, "#,
    r#"$bitrateLow: Int, $bitrateHigh: Int, $validationOnly: Boolean) {\n"#,
    r#"  webfront_playlistUrl(\n    code: $code\n    playMode: $playMode\n"#,
    r#"    bitrateLow: $bitrateLow\n    bitrateHigh: $bitrateHigh\n"#,
    r#"    validationOnly: $validationOnly\n  ) {\n    subTitle\n    playToken\n"#,
    r#"    playTokenHash\n    beaconSpan\n    result {\n      errorCode\n"#,
    r#"      errorMessage\n      __typename\n    }\n    resultStatus\n"#,
    r#"    licenseExpireDate\n    urlInfo {\n      code\n      startPoint\n"#,
    r#"      resumePoint\n      endPoint\n      endrollStartPosition\n      holderId\n"#,
    r#"      saleTypeCode\n      sceneSearchList {\n        IMS_AD1\n        IMS_L\n"#,
    r#"        IMS_M\n        IMS_S\n        __typename\n      }\n"#,
    r#"      movieProfile {\n        cdnId\n        type\n        playlistUrl\n"#,
    r#"        movieAudioList {\n          audioType\n          __typename\n        }\n"#,
    r#"        licenseUrlList {\n          type\n          licenseUrl\n"#,
    r#"          __typename\n        }\n        __typename\n      }\n"#,
    r#"      umcContentId\n      movieSecurityLevelCode\n      captionFlg\n"#,
    r#"      dubFlg\n      commodityCode\n      movieAudioList {\n        audioType\n"#,
    r#"        __typename\n      }\n      moviePartsPositionList {\n        type\n"#,
    r#"        fromSeconds\n        endSeconds\n        hasRemainingPart\n"#,
    r#"        __typename\n      }\n      __typename\n    }\n    __typename\n  }\n}\n"}"#,
);

/// Platform specification for U-NEXT.
pub const SPEC: Platform = Platform {
    id: "u-next",
    name: "U-NEXT",
    probe,
};

/// Probes U-NEXT over `family`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        let url = |real: &str| parse::endpoint(base_url, SPEC.id, "check", real);
        let req = HttpRequest::post(
            family,
            url("https://cc.unext.jp/"),
            PLAYLIST_QUERY.as_bytes().to_vec(),
        )
        .with_header("content-type", "application/json")
        .with_header("origin", "https://video.unext.jp")
        .with_header("referer", "https://video.unext.jp/");
        match parse::send(ctx, req).await {
            Ok(res) => parse(&res),
            Err(verdict) => verdict,
        }
    })
}

/// Parses U-NEXT's HTTP response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    if let Some(val) = parse::json(response) {
        let as_code = |v: &serde_json::Value| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        };
        let status = [
            "/data/webfront_playlistUrl/resultStatus",
            "/data/result_status",
            "/code",
        ]
        .into_iter()
        .find_map(|ptr| val.pointer(ptr).and_then(as_code));
        let ok_http = (200..300).contains(&response.status);
        return match status {
            // 467 is U-NEXT's overseas IP restriction code; 470/475/200/0 mean domestic auth check.
            Some(467) => Verdict::no(),
            Some(0 | 200 | 470 | 475) if ok_http => Verdict::yes_in("JP"),
            Some(code) => Verdict::Unexpected(format!("unrecognized U-NEXT code {code}")),
            None if ok_http => parse::no_region(),
            None => Verdict::Unexpected(format!("HTTP {}", response.status)),
        };
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}
