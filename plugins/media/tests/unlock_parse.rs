//! Parser tests across all five regional modules, anti-bot WAF challenges, rate limits, and
//! unexpected responses.

use bench_api::HttpError;
use bench_api::HttpResponse;
use plugin_media::unlock::Restriction;
use plugin_media::unlock::Verdict;
use plugin_media::unlock::parse;
use plugin_media::unlock::parse::americas;
use plugin_media::unlock::parse::asia;
use plugin_media::unlock::parse::europe;
use plugin_media::unlock::parse::global;
use plugin_media::unlock::parse::oceania;

fn response(status: u16, final_url: &str, headers: &[(&str, &str)], body: &str) -> HttpResponse {
    HttpResponse {
        status,
        final_url: final_url.to_owned(),
        headers: headers
            .iter()
            .map(|(k, v)| ((*k).to_ascii_lowercase(), (*v).to_owned()))
            .collect(),
        body: body.as_bytes().to_vec(),
    }
}

#[test]
fn gate_classifies_turnstile_waf_rate_limits_and_transport_errors_without_bypass() {
    let cf_header = response(
        403,
        "https://www.netflix.com/title/80018499",
        &[("cf-mitigated", "challenge")],
        "<html></html>",
    );
    assert_eq!(parse::gate(&cf_header), Some(Verdict::BlockedWaf));

    let turnstile_html = response(
        403,
        "https://www.max.com/",
        &[],
        concat!(
            "<html><head><title>Just a moment...</title></head>",
            "<script src=\"https://challenges.cloudflare.com/turnstile/v0/api.js\"></script>",
            "</html>"
        ),
    );
    assert_eq!(parse::gate(&turnstile_html), Some(Verdict::BlockedWaf));

    let rate_limited = response(429, "https://api.abema.io/v1/ip/check", &[], "");
    assert_eq!(parse::gate(&rate_limited), Some(Verdict::RateLimited));

    let server_err = response(502, "https://api.abema.io/v1/ip/check", &[], "Bad Gateway");
    assert_eq!(
        parse::gate(&server_err),
        Some(Verdict::Unexpected("HTTP 502".to_owned()))
    );

    assert_eq!(
        parse::error_verdict(&HttpError::NoAddress),
        Verdict::NoAddress
    );
    assert_eq!(
        parse::error_verdict(&HttpError::TimedOut),
        Verdict::TimedOut
    );
}

#[test]
fn global_parsers_distinguish_full_originals_regions_and_restrictions() {
    // Netflix: full unlock, originals only, proxy detected, and blocked.
    let nf_us = response(200, "https://www.netflix.com/us/title/81280792", &[], "ok");
    let nf_orig = response(
        200,
        "https://www.netflix.com/de-en/title/80018499",
        &[],
        "ok",
    );
    let nf_404 = response(
        404,
        "https://www.netflix.com/title/81280792",
        &[],
        "Not Available",
    );
    let nf_proxy = response(
        200,
        "https://www.netflix.com/title/80018499",
        &[],
        "NSES-UH4",
    );
    assert_eq!(global::netflix(&nf_us, &nf_orig), Verdict::yes_in("US"));
    assert_eq!(
        global::netflix(&nf_404, &nf_orig),
        Verdict::OriginalsOnly {
            region: Some("DE".to_owned())
        }
    );
    assert_eq!(global::netflix(&nf_404, &nf_404), Verdict::no());
    assert_eq!(
        global::netflix(&nf_404, &nf_proxy),
        Verdict::No {
            restriction: Some(Restriction::ProxyDetected)
        }
    );

    // YouTube Premium: unlocked with region, Mainland China redirect, and unavailable.
    let yt_jp = response(
        200,
        "https://www.youtube.com/premium",
        &[],
        r#"{"INNERTUBE_CONTEXT_GL":"JP"} YouTube and YouTube Music ad-free"#,
    );
    assert_eq!(global::youtube(&yt_jp), Verdict::yes_in("JP"));
    let yt_cn = response(200, "https://www.google.cn/", &[], "");
    assert_eq!(
        global::youtube(&yt_cn),
        Verdict::No {
            restriction: Some(Restriction::ChinaMainland)
        }
    );
    let yt_no = response(
        200,
        "https://www.youtube.com/premium",
        &[],
        "YouTube Premium is not available in your country",
    );
    assert_eq!(global::youtube(&yt_no), Verdict::no());

    // Disney+, Prime Video, Apple TV+, TikTok, Spotify, DAZN, Crunchyroll, Twitch, Rakuten, MUBI.
    let disney_web = response(200, "https://www.disneyplus.com/", &[], "");
    let disney_api = response(
        200,
        "https://disney.api.edge.bamgrid.com/v1/public/graphql",
        &[],
        concat!(
            r#"{"extensions":{"sdk":{"session":{"location":"#,
            r#"{"countryCode":"SG","inSupportedLocation":true}}}}}"#
        ),
    );
    assert_eq!(
        global::disney(&disney_web, Some(&disney_api)),
        Verdict::yes_in("SG")
    );
    let disney_unavail = response(
        302,
        "https://www.disneyplus.com/",
        &[("location", "https://www.disneyplus.com/unavailable")],
        "",
    );
    assert_eq!(
        global::disney_web_early(&disney_unavail),
        Some(Verdict::no())
    );

    let pv = response(
        200,
        "https://www.primevideo.com/",
        &[],
        r#"<script>{"currentTerritory":"GB"}</script>"#,
    );
    assert_eq!(global::prime_video(&pv), Verdict::yes_in("GB"));

    let atv = response(200, "https://tv.apple.com/au", &[], "");
    assert_eq!(global::apple_tv(&atv), Verdict::yes_in("AU"));

    let tt_ok = response(
        200,
        "https://www.tiktok.com/cdn-cgi/trace",
        &[],
        "fl=1\nloc=TW\n",
    );
    let tt_hk = response(
        200,
        "https://www.tiktok.com/cdn-cgi/trace",
        &[],
        "fl=1\nloc=HK\n",
    );
    assert_eq!(global::tiktok(&tt_ok), Verdict::yes_in("TW"));
    assert_eq!(global::tiktok(&tt_hk), Verdict::no());

    let sp = response(
        200,
        "https://spclient.wg.spotify.com/signup/public/v1/account",
        &[],
        r#"{"status":1,"country":"US","is_country_launched":true}"#,
    );
    assert_eq!(global::spotify(&sp), Verdict::yes_in("US"));

    let dazn = response(
        200,
        "https://startup.core.indazn.com/misl/v5/Startup",
        &[],
        r#"{"Region":{"isAllowed":true,"GeolocatedCountry":"de"}}"#,
    );
    assert_eq!(global::dazn(&dazn), Verdict::yes_in("DE"));

    let cr = response(
        200,
        "https://www.crunchyroll.com/auth/v1/token",
        &[],
        r#"{"access_token":"tok","country":"CA"}"#,
    );
    assert_eq!(global::crunchyroll(&cr), Verdict::yes_in("CA"));

    let tw = response(
        200,
        "https://gql.twitch.tv/gql",
        &[],
        r#"{"data":{"requestInfo":{"countryCode":"KR"}}}"#,
    );
    assert_eq!(global::twitch(&tw), Verdict::yes_in("KR"));

    let rk = response(
        200,
        "https://gizmo.rakuten.tv/v3/me/start",
        &[],
        r#"{"data":{"market_code":"es"}}"#,
    );
    assert_eq!(global::rakuten_tv(&rk), Verdict::yes_in("ES"));

    let mubi = response(
        200,
        "https://api.mubi.com/v3/country",
        &[],
        r#"{"code":"FR","available":true}"#,
    );
    assert_eq!(global::mubi(&mubi), Verdict::yes_in("FR"));
}

#[test]
fn asia_parsers_cover_hk_mo_tw_japan_korea_and_singapore_services() {
    let bili_ok = response(200, "https://api.bilibili.com/x", &[], r#"{"code":0}"#);
    let bili_no = response(200, "https://api.bilibili.com/x", &[], r#"{"code":-10403}"#);
    assert_eq!(asia::bilibili(&bili_ok, &bili_ok), Verdict::yes_in("TW"));
    assert_eq!(
        asia::bilibili(&bili_no, &bili_ok),
        Verdict::Yes {
            region: Some("HK/MO".to_owned())
        }
    );
    assert_eq!(asia::bilibili(&bili_no, &bili_no), Verdict::no());

    let baha_dev = response(
        200,
        "https://ani.gamer.com.tw/ajax/getdeviceid.php",
        &[],
        r#"{"deviceid":"dev123"}"#,
    );
    assert_eq!(
        asia::bahamut_device_id(&baha_dev).as_deref(),
        Some("dev123")
    );
    let baha_tok = response(
        200,
        "https://ani.gamer.com.tw/ajax/token.php",
        &[],
        r#"{"animeSn":37783}"#,
    );
    assert_eq!(asia::bahamut_token(&baha_tok), Verdict::yes_in("TW"));

    let viu_sg = response(200, "https://www.viu.com/ott/sg/en-us", &[], "");
    let viu_no = response(200, "https://www.viu.com/ott/no-service/", &[], "");
    assert_eq!(asia::viu(&viu_sg), Verdict::yes_in("SG"));
    assert_eq!(asia::viu(&viu_no), Verdict::no());

    assert_eq!(
        asia::mytvsuper(&response(200, "https://x", &[], r#"{"region":1}"#)),
        Verdict::yes_in("HK")
    );
    assert_eq!(
        asia::hoy_tv(&response(200, "https://x", &[], r#"{"code":200}"#)),
        Verdict::yes_in("HK")
    );
    assert_eq!(
        asia::kktv(&response(
            200,
            "https://x",
            &[],
            r#"{"data":{"country":"TW","is_allowed":true}}"#
        )),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        asia::litv(&response(200, "https://x", &[], r#"{"errorMessage":null}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        asia::myvideo(&response(200, "https://www.myvideo.net.tw/", &[], "ok")),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        asia::linetv(&response(200, "https://x", &[], r#"{"eps_info":{}}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        asia::friday_video(&response(200, "https://x", &[], r#"{"code":0}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        asia::catchplay(&response(200, "https://x", &[], r#"{"isoCode":"ID"}"#)),
        Verdict::yes_in("ID")
    );
    assert_eq!(
        asia::hamivideo(&response(200, "https://x", &[], r#"{"code":"06001-107"}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        asia::abema(&response(
            200,
            "https://x",
            &[],
            r#"{"isoCountryCode":"JP"}"#
        )),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        asia::dmm(&response(200, "https://x", &[], r#"{"is_foreign":false}"#)),
        Verdict::yes_in("JP")
    );
    let tver_c = response(
        200,
        "https://x",
        &[],
        r#"{"result":{"platform_uid":"u1","platform_token":"t1"}}"#,
    );
    assert_eq!(
        asia::tver_credentials(&tver_c),
        Some(("u1".to_owned(), "t1".to_owned()))
    );
    assert_eq!(
        asia::tver_home(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        asia::niconico(&response(200, "https://x", &[], "watch-data")),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        asia::u_next(&response(
            200,
            "https://x",
            &[],
            r#"{"data":{"result_status":475}}"#
        )),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        asia::u_next(&response(
            200,
            "https://x",
            &[],
            r#"{"data":{"result_status":467}}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        asia::lemino(&response(400, "https://x", &[], "{}")),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        asia::nhk_plus(&response(200, "https://x", &[], r#"{"is_domestic":true}"#)),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        asia::radiko(&response(200, "https://x", &[], "JP13 TOKYO")),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        asia::wavve(&response(200, "https://x", &[], r#"{"resultcode":"200"}"#)),
        Verdict::yes_in("KR")
    );
    assert_eq!(
        asia::tving(&response(
            200,
            "https://x",
            &[],
            r#"{"header":{"code":"000"}}"#
        )),
        Verdict::yes_in("KR")
    );
    assert_eq!(
        asia::coupang_play(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("KR")
    );
    assert_eq!(
        asia::mewatch(&response(200, "https://x", &[], "[]")),
        Verdict::yes_in("SG")
    );
}

#[test]
fn europe_americas_and_oceania_parsers_cover_every_regional_service() {
    // Europe (14)
    assert_eq!(
        europe::bbc_iplayer(&response(200, "https://x", &[], r#"{"media":[]}"#)),
        Verdict::yes_in("GB")
    );
    assert_eq!(
        europe::bbc_iplayer(&response(
            200,
            "https://x",
            &[],
            r#"{"result":"geolocation"}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        europe::channel_4(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("GB")
    );
    assert_eq!(
        europe::itv_x(&response(401, "https://x", &[], "")),
        Verdict::yes_in("GB")
    );
    assert_eq!(
        europe::sky_go(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("GB")
    );
    assert_eq!(
        europe::zdf(&response(200, "https://x", &[], "DE\n")),
        Verdict::yes_in("DE")
    );
    assert_eq!(
        europe::joyn(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("DE")
    );
    assert_eq!(
        europe::rtl_plus(&response(200, "https://plus.rtl.de/", &[], "ok")),
        Verdict::yes_in("DE")
    );
    assert_eq!(
        europe::canal_plus(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("FR")
    );
    assert_eq!(
        europe::molotov(&response(200, "https://x", &[], r#"{"is_france":true}"#)),
        Verdict::yes_in("FR")
    );
    assert_eq!(
        europe::france_tv(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("FR")
    );
    assert_eq!(
        europe::rai_play(&response(200, "https://x", &[], "<video/>")),
        Verdict::yes_in("IT")
    );
    assert_eq!(
        europe::movistar_plus(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("ES")
    );
    assert_eq!(
        europe::nlziet(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("NL")
    );
    assert_eq!(
        europe::viaplay(&response(
            302,
            "https://checkout.viaplay.com/",
            &[("location", "/se/")],
            ""
        )),
        Verdict::yes_in("SE")
    );

    // Americas (10)
    assert_eq!(
        americas::hulu(&response(
            200,
            "https://www.hulu.com/welcome",
            &[],
            "welcome"
        )),
        Verdict::yes_in("US")
    );
    assert_eq!(
        americas::max(&response(
            302,
            "https://www.max.com/",
            &[("location", "/geo-availability")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        americas::max(&response(
            200,
            "https://www.max.com/",
            &[("x-wbd-country", "BR")],
            ""
        )),
        Verdict::yes_in("BR")
    );
    assert_eq!(
        americas::peacock(&response(
            200,
            "https://www.peacocktv.com/",
            &[("x-skyott-territory", "US")],
            ""
        )),
        Verdict::yes_in("US")
    );
    assert_eq!(
        americas::paramount_plus(&response(
            302,
            "https://www.paramountplus.com/",
            &[("location", "/intl/")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        americas::starz(&response(
            200,
            "https://x",
            &[],
            r#"{"isAllowedCountry":true,"country":"US"}"#
        )),
        Verdict::yes_in("US")
    );
    assert_eq!(
        americas::espn_plus(&response(200, "https://plus.espn.com/", &[], "ok")),
        Verdict::yes_in("US")
    );
    assert_eq!(
        americas::sling_tv(&response(200, "https://www.sling.com/", &[], "ok")),
        Verdict::yes_in("US")
    );
    assert_eq!(
        americas::pluto_tv(&response(
            200,
            "https://x",
            &[],
            r#"{"session":{"countryCode":"US"}}"#
        )),
        Verdict::yes_in("US")
    );
    assert_eq!(
        americas::crave(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("CA")
    );
    assert_eq!(
        americas::cbc_gem(&response(200, "https://x", &[], r#"{"errorCode":0}"#)),
        Verdict::yes_in("CA")
    );

    // Oceania (8)
    assert_eq!(
        oceania::stan(&response(
            400,
            "https://x",
            &[],
            r#"{"errors":["Invalid"]}"#
        )),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        oceania::stan(&response(
            403,
            "https://x",
            &[],
            r#"{"errors":["StreamCo.Concurrency.GeoBlocked"]}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        oceania::binge(&response(401, "https://x", &[], "{}")),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        oceania::kayo_sports(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        oceania::nine_now(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        oceania::seven_plus(&response(200, "https://x", &[], r#"{"token":"abc"}"#)),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        oceania::abc_iview(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        oceania::tvnz_plus(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("NZ")
    );
    assert_eq!(
        oceania::threenow(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("NZ")
    );
}
