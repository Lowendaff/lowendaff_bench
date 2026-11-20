//! Per-platform parser tests across all 68 streaming modules, anti-bot WAF challenges, rate
//! limits, and unexpected responses.

use bench_api::HttpError;
use bench_api::HttpResponse;
use plugin_ip::media::Restriction;
use plugin_ip::media::Verdict;
use plugin_ip::media::parse;
use plugin_ip::media::platforms::abc_iview;
use plugin_ip::media::platforms::abema;
use plugin_ip::media::platforms::apple_tv;
use plugin_ip::media::platforms::bahamut;
use plugin_ip::media::platforms::bbc_iplayer;
use plugin_ip::media::platforms::bilibili;
use plugin_ip::media::platforms::binge;
use plugin_ip::media::platforms::canal_plus;
use plugin_ip::media::platforms::catchplay;
use plugin_ip::media::platforms::cbc_gem;
use plugin_ip::media::platforms::channel_4;
use plugin_ip::media::platforms::coupang_play;
use plugin_ip::media::platforms::crave;
use plugin_ip::media::platforms::crunchyroll;
use plugin_ip::media::platforms::dazn;
use plugin_ip::media::platforms::disney;
use plugin_ip::media::platforms::dmm;
use plugin_ip::media::platforms::espn_plus;
use plugin_ip::media::platforms::france_tv;
use plugin_ip::media::platforms::friday_video;
use plugin_ip::media::platforms::hamivideo;
use plugin_ip::media::platforms::hoy_tv;
use plugin_ip::media::platforms::hulu;
use plugin_ip::media::platforms::itv_x;
use plugin_ip::media::platforms::joyn;
use plugin_ip::media::platforms::kayo_sports;
use plugin_ip::media::platforms::kktv;
use plugin_ip::media::platforms::lemino;
use plugin_ip::media::platforms::linetv;
use plugin_ip::media::platforms::litv;
use plugin_ip::media::platforms::max;
use plugin_ip::media::platforms::mewatch;
use plugin_ip::media::platforms::molotov;
use plugin_ip::media::platforms::movistar_plus;
use plugin_ip::media::platforms::mubi;
use plugin_ip::media::platforms::mytvsuper;
use plugin_ip::media::platforms::myvideo;
use plugin_ip::media::platforms::netflix;
use plugin_ip::media::platforms::nhk_plus;
use plugin_ip::media::platforms::niconico;
use plugin_ip::media::platforms::nine_now;
use plugin_ip::media::platforms::nlziet;
use plugin_ip::media::platforms::paramount_plus;
use plugin_ip::media::platforms::peacock;
use plugin_ip::media::platforms::pluto_tv;
use plugin_ip::media::platforms::prime_video;
use plugin_ip::media::platforms::radiko;
use plugin_ip::media::platforms::rai_play;
use plugin_ip::media::platforms::rakuten_tv;
use plugin_ip::media::platforms::rtl_plus;
use plugin_ip::media::platforms::seven_plus;
use plugin_ip::media::platforms::sky_go;
use plugin_ip::media::platforms::sling_tv;
use plugin_ip::media::platforms::spotify;
use plugin_ip::media::platforms::stan;
use plugin_ip::media::platforms::starz;
use plugin_ip::media::platforms::threenow;
use plugin_ip::media::platforms::tiktok;
use plugin_ip::media::platforms::tver;
use plugin_ip::media::platforms::tving;
use plugin_ip::media::platforms::tvnz_plus;
use plugin_ip::media::platforms::twitch;
use plugin_ip::media::platforms::u_next;
use plugin_ip::media::platforms::viaplay;
use plugin_ip::media::platforms::viu;
use plugin_ip::media::platforms::wavve;
use plugin_ip::media::platforms::youtube;
use plugin_ip::media::platforms::zdf;

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

    let datadome_allowed = response(
        200,
        "https://www.netflix.com/",
        &[("x-datadome", "protected")],
        "ok",
    );
    assert_eq!(parse::gate(&datadome_allowed), None);

    let datadome_block = response(
        403,
        "https://www.netflix.com/",
        &[("x-dd-b", "1")],
        "<html></html>",
    );
    assert_eq!(parse::gate(&datadome_block), Some(Verdict::BlockedWaf));

    let datadome_captcha = response(
        403,
        "https://www.example.com/",
        &[],
        "<script src=\"https://geo.captcha-delivery.com/captcha/\"></script>",
    );
    assert_eq!(parse::gate(&datadome_captcha), Some(Verdict::BlockedWaf));

    let akamai_block = response(
        403,
        "https://ssl.zdf.de/geo/de/geo.txt",
        &[],
        "<HTML><HEAD><TITLE>Access Denied</TITLE></HEAD><BODY>errors.edgesuite.net</BODY></HTML>",
    );
    assert_eq!(parse::gate(&akamai_block), Some(Verdict::BlockedWaf));

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
fn global_platform_modules_distinguish_full_originals_regions_and_restrictions() {
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
    assert_eq!(netflix::parse(&nf_us, &nf_orig), Verdict::yes_in("US"));
    assert_eq!(
        netflix::parse(&nf_404, &nf_orig),
        Verdict::OriginalsOnly {
            region: Some("DE".to_owned())
        }
    );
    assert_eq!(netflix::parse(&nf_404, &nf_404), Verdict::no());
    assert_eq!(
        netflix::parse(&nf_404, &nf_proxy),
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
    assert_eq!(youtube::parse(&yt_jp), Verdict::yes_in("JP"));
    let yt_cn = response(200, "https://www.google.cn/", &[], "");
    assert_eq!(
        youtube::parse(&yt_cn),
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
    assert_eq!(youtube::parse(&yt_no), Verdict::no());

    // Disney+, Prime Video, Apple TV+, TikTok, Spotify, DAZN, Crunchyroll, Twitch, Rakuten, MUBI.
    let disney_web = response(
        200,
        "https://www.disneyplus.com/",
        &[],
        r#"{"countryCode":"SG"}"#,
    );
    assert_eq!(disney::parse(&disney_web), Verdict::yes_in("SG"));
    assert_eq!(
        disney::parse(&response(200, "https://www.disneyplus.com/", &[], "")),
        parse::no_region()
    );
    let disney_unavail = response(
        302,
        "https://www.disneyplus.com/",
        &[("location", "https://www.disneyplus.com/unavailable")],
        "",
    );
    assert_eq!(disney::web_early(&disney_unavail), Some(Verdict::no()));

    let pv = response(
        200,
        "https://www.primevideo.com/",
        &[],
        r#"<script>{"currentTerritory":"GB"}</script>"#,
    );
    assert_eq!(prime_video::parse(&pv), Verdict::yes_in("GB"));

    let atv = response(200, "https://tv.apple.com/au", &[], "");
    assert_eq!(apple_tv::parse(&atv), Verdict::yes_in("AU"));

    let tt_ok = response(
        200,
        "https://www.tiktok.com/explore",
        &[],
        concat!(
            r#"{"__DEFAULT_SCOPE__":{"webapp.app-context":"#,
            r#"{"language":"en","region":"TW","appId":1233}}}"#
        ),
    );
    let tt_hk = response(
        200,
        "https://www.tiktok.com/explore",
        &[],
        concat!(
            r#"{"__DEFAULT_SCOPE__":{"webapp.app-context":"#,
            r#"{"language":"en","region":"HK","appId":1233}}}"#
        ),
    );
    assert_eq!(tiktok::parse(&tt_ok), Verdict::yes_in("TW"));
    assert_eq!(tiktok::parse(&tt_hk), Verdict::no());
    assert_eq!(
        tiktok::parse(&response(
            200,
            "https://www.tiktok.com/explore",
            &[],
            "<html></html>"
        )),
        Verdict::Unexpected("missing TikTok region".to_owned())
    );

    let sp = response(
        200,
        "https://spclient.wg.spotify.com/signup/public/v1/account",
        &[],
        r#"{"status":1,"country":"US","is_country_launched":true}"#,
    );
    assert_eq!(spotify::parse(&sp), Verdict::yes_in("US"));

    let dazn_res = response(
        200,
        "https://startup.core.indazn.com/misl/v5/Startup",
        &[],
        r#"{"Region":{"isAllowed":true,"GeolocatedCountry":"de"}}"#,
    );
    assert_eq!(dazn::parse(&dazn_res), Verdict::yes_in("DE"));

    let cr = response(
        200,
        "https://www.crunchyroll.com/auth/v1/token",
        &[],
        r#"{"access_token":"tok","country":"CA"}"#,
    );
    assert_eq!(crunchyroll::parse(&cr), Verdict::yes_in("CA"));

    let tw = response(
        200,
        "https://gql.twitch.tv/gql",
        &[],
        r#"{"data":{"requestInfo":{"countryCode":"KR"}}}"#,
    );
    assert_eq!(twitch::parse(&tw), Verdict::yes_in("KR"));

    let rk = response(
        200,
        "https://gizmo.rakuten.tv/v3/me/start",
        &[],
        r#"{"data":{"market_code":"es"}}"#,
    );
    assert_eq!(rakuten_tv::parse(&rk), Verdict::yes_in("ES"));

    let mubi_res = response(
        200,
        "https://api.mubi.com/v3/country",
        &[],
        r#"{"code":"FR","available":true}"#,
    );
    assert_eq!(mubi::parse(&mubi_res), Verdict::yes_in("FR"));
}

#[test]
fn asia_platform_modules_cover_hk_mo_tw_japan_korea_and_singapore_services() {
    let bili_ok = response(200, "https://api.bilibili.com/x", &[], r#"{"code":0}"#);
    let bili_no = response(200, "https://api.bilibili.com/x", &[], r#"{"code":-10403}"#);
    assert_eq!(bilibili::parse(&bili_ok, &bili_ok), Verdict::yes_in("TW"));
    assert_eq!(
        bilibili::parse(&bili_no, &bili_ok),
        Verdict::Yes {
            region: Some("HK/MO".to_owned())
        }
    );
    assert_eq!(bilibili::parse(&bili_no, &bili_no), Verdict::no());

    let baha_dev = response(
        200,
        "https://ani.gamer.com.tw/ajax/getdeviceid.php",
        &[],
        r#"{"deviceid":"dev123"}"#,
    );
    assert_eq!(bahamut::device_id(&baha_dev).as_deref(), Some("dev123"));
    let baha_tok = response(
        200,
        "https://ani.gamer.com.tw/ajax/token.php",
        &[],
        r#"{"animeSn":37783}"#,
    );
    assert_eq!(bahamut::parse(&baha_tok), Verdict::yes_in("TW"));

    let viu_sg = response(200, "https://www.viu.com/ott/sg/en-us", &[], "");
    let viu_no = response(200, "https://www.viu.com/ott/no-service/", &[], "");
    assert_eq!(viu::parse(&viu_sg), Verdict::yes_in("SG"));
    assert_eq!(viu::parse(&viu_no), Verdict::no());

    assert_eq!(
        mytvsuper::parse(&response(200, "https://x", &[], r#"{"region":1}"#)),
        Verdict::yes_in("HK")
    );
    assert_eq!(
        hoy_tv::parse(&response(200, "https://x", &[], r#"{"code":200}"#)),
        parse::no_region()
    );
    assert_eq!(
        hoy_tv::parse(&response(200, "https://x", &[], r#"{"code":403}"#)),
        Verdict::no()
    );
    assert_eq!(
        kktv::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"data":{"country":"TW","is_allowed":true}}"#
        )),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        litv::parse(&response(200, "https://x", &[], r#"{"errorMessage":null}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        myvideo::parse(&response(200, "https://www.myvideo.net.tw/", &[], "ok")),
        parse::no_region()
    );
    assert_eq!(
        myvideo::parse(&response(
            200,
            "https://www.myvideo.net.tw/serviceAreaBlock",
            &[],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        linetv::parse(&response(200, "https://x", &[], r#"{"eps_info":{}}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        friday_video::parse(&response(200, "https://x", &[], r#"{"code":0}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        friday_video::parse(&response(
            200,
            "https://x",
            &[],
            concat!(
                "{\"code\":\"9001\",",
                "\"message\":\"\u{670d}\u{52d9}\u{5fd9}\u{788c}\u{4e2d}\",",
                "\"status\":500}"
            )
        )),
        Verdict::Unexpected(
            "streaming code 9001: \u{670d}\u{52d9}\u{5fd9}\u{788c}\u{4e2d}".to_owned()
        )
    );
    assert_eq!(
        catchplay::parse(&response(200, "https://x", &[], r#"{"isoCode":"ID"}"#)),
        Verdict::yes_in("ID")
    );
    assert_eq!(
        hamivideo::parse(&response(200, "https://x", &[], r#"{"code":"06001-107"}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        abema::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"isoCountryCode":"JP"}"#
        )),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        dmm::parse(&response(200, "https://x", &[], r#"{"is_foreign":false}"#)),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        dmm::parse(&response(200, "https://x", &[], r#"{"is_foreign":true}"#)),
        Verdict::no()
    );
    let tver_c = response(
        200,
        "https://x",
        &[],
        r#"{"result":{"platform_uid":"u1","platform_token":"t1"}}"#,
    );
    assert_eq!(
        tver::credentials(&tver_c),
        Some(("u1".to_owned(), "t1".to_owned()))
    );
    assert_eq!(
        tver::parse(&response(200, "https://x", &[], "{}")),
        parse::no_region()
    );
    assert_eq!(
        tver::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"result":{"platform_uid":"u1"}}"#
        )),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        tver::parse(&response(
            403,
            "https://x",
            &[],
            r#"{"message":"outside japan"}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        niconico::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"meta":{"status":200}}"#
        )),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        niconico::parse(&response(
            400,
            "https://x",
            &[],
            concat!(
                r#"{"meta":{"status":400,"errorCode":"FORBIDDEN"},"#,
                r#""data":{"reasonCode":"DOMESTIC_VIDEO"}}"#
            )
        )),
        Verdict::no()
    );
    assert_eq!(
        niconico::parse(&response(
            404,
            "https://x",
            &[],
            r#"{"meta":{"status":404,"errorCode":"NOT_FOUND"}}"#
        )),
        Verdict::Unexpected("watch API status 404".to_owned())
    );
    assert_eq!(
        u_next::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"data":{"result_status":475}}"#
        )),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        u_next::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"data":{"result_status":467}}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        lemino::parse(&response(400, "https://x", &[], "{}")),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        nhk_plus::parse(&response(200, "https://x", &[], r#"{"is_domestic":true}"#)),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        nhk_plus::parse(&response(
            302,
            "https://x",
            &[("location", "https://www.web.nhk/")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        radiko::parse(&response(200, "https://x", &[], "JP13 TOKYO")),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        wavve::parse(&response(200, "https://x", &[], r#"{"resultcode":"200"}"#)),
        Verdict::yes_in("KR")
    );
    assert_eq!(
        wavve::parse(&response(
            200,
            "https://x",
            &[],
            concat!(
                "{\"warning_kr\":\"\u{d604}\u{c7ac} \u{b2f9}\u{c0ac}\u{c758} ",
                "\u{c11c}\u{be44}\u{c2a4}\u{b97c} \u{bd88}\u{bc95}\u{c801}\u{c73c}\u{b85c} ",
                "\u{c774}\u{c6a9}\u{d558}\u{ace0} \u{c788}\u{c2b5}\u{b2c8}\u{b2e4}\"}"
            )
        )),
        Verdict::No {
            restriction: Some(Restriction::ProxyDetected)
        }
    );
    assert_eq!(
        tving::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"header":{"code":"000"},"body":{"content":{"videoId":"1"}}}"#
        )),
        Verdict::yes_in("KR")
    );
    assert_eq!(
        tving::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"header":{"message":"OK","status":200},"body":{"result":null,"content":{}}}"#
        )),
        parse::no_region()
    );
    assert_eq!(
        coupang_play::parse(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("KR")
    );
    assert_eq!(
        mewatch::parse(&response(200, "https://x", &[], "[]")),
        Verdict::yes_in("SG")
    );
}

#[test]
fn europe_americas_and_oceania_platform_modules_cover_every_regional_service() {
    // Europe (14)
    assert_eq!(
        bbc_iplayer::parse(&response(200, "https://x", &[], r#"{"media":[]}"#)),
        Verdict::yes_in("GB")
    );
    assert_eq!(
        bbc_iplayer::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"result":"geolocation"}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        channel_4::parse(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("GB")
    );
    assert_eq!(
        itv_x::parse(&response(401, "https://x", &[], "")),
        Verdict::yes_in("GB")
    );
    assert_eq!(
        sky_go::parse(&response(200, "https://x", &[], "{}")),
        parse::no_region()
    );
    assert_eq!(
        sky_go::parse(&response(403, "https://x", &[], "geoblocked")),
        Verdict::no()
    );
    assert_eq!(
        zdf::parse(&response(200, "https://x", &[], "DE\n")),
        Verdict::yes_in("DE")
    );
    assert_eq!(
        zdf::parse(&response(200, "https://x", &[], "US\n")),
        Verdict::no()
    );
    assert_eq!(
        zdf::parse(&response(200, "https://x", &[], "XX\n")),
        Verdict::no()
    );
    assert_eq!(
        zdf::parse(&response(200, "https://x", &[], "not-a-country")),
        parse::no_region()
    );
    assert_eq!(
        joyn::parse(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("DE")
    );
    assert_eq!(
        rtl_plus::parse(&response(200, "https://plus.rtl.de/", &[], "ok")),
        parse::no_region()
    );
    assert_eq!(
        rtl_plus::parse(&response(200, "https://plus.rtl.de/", &[], "geoblocking")),
        Verdict::no()
    );
    assert_eq!(
        canal_plus::parse(&response(200, "https://x", &[], "ok")),
        parse::no_region()
    );
    assert_eq!(
        canal_plus::parse(&response(
            302,
            "https://x",
            &[("location", "https://boutique.canalplus.com/geo")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        molotov::parse(&response(200, "https://x", &[], r#"{"is_france":true}"#)),
        Verdict::yes_in("FR")
    );
    assert_eq!(
        france_tv::parse(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("FR")
    );
    assert_eq!(
        rai_play::parse(&response(200, "https://x", &[], "<video/>")),
        Verdict::yes_in("IT")
    );
    assert_eq!(
        movistar_plus::parse(&response(200, "https://x", &[], "ok")),
        parse::no_region()
    );
    assert_eq!(
        nlziet::parse(&response(200, "https://x", &[], "ok")),
        parse::no_region()
    );
    assert_eq!(
        nlziet::parse(&response(
            302,
            "https://x",
            &[("location", "https://app.nlziet.nl/geo")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        viaplay::parse(&response(
            302,
            "https://checkout.viaplay.com/",
            &[("location", "/se/")],
            ""
        )),
        Verdict::yes_in("SE")
    );

    // Americas (10)
    assert_eq!(
        hulu::parse(&response(
            200,
            "https://www.hulu.com/welcome",
            &[],
            "welcome"
        )),
        parse::no_region()
    );
    assert_eq!(
        hulu::parse(&response(
            302,
            "https://www.hulu.com/welcome",
            &[("location", "https://www.disneyplus.com/")],
            ""
        )),
        Verdict::Unexpected("redirected to Disney+".to_owned())
    );
    assert_eq!(
        max::parse(&response(
            302,
            "https://www.max.com/",
            &[("location", "/geo-availability")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        max::parse(&response(
            200,
            "https://www.max.com/",
            &[("x-wbd-country", "BR")],
            ""
        )),
        Verdict::yes_in("BR")
    );
    assert_eq!(
        max::parse(&response(
            301,
            "https://www.max.com/",
            &[
                ("location", "https://www.hbomax.com/"),
                ("set-cookie", "countryCode=EU; path=/; domain=.hbomax.com"),
            ],
            ""
        )),
        Verdict::yes_in("EU")
    );
    assert_eq!(
        max::parse(&response(
            301,
            "https://www.max.com/",
            &[("location", "https://www.hbomax.com/")],
            ""
        )),
        parse::no_region()
    );
    assert_eq!(
        peacock::parse(&response(
            200,
            "https://www.peacocktv.com/",
            &[("x-skyott-territory", "US")],
            ""
        )),
        Verdict::yes_in("US")
    );
    assert_eq!(
        peacock::parse(&response(200, "https://www.peacocktv.com/", &[], "")),
        parse::no_region()
    );
    assert_eq!(
        paramount_plus::parse(&response(
            302,
            "https://www.paramountplus.com/",
            &[("location", "/intl/")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        starz::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"isAllowedCountry":true,"country":"US"}"#
        )),
        Verdict::yes_in("US")
    );
    assert_eq!(
        espn_plus::parse(&response(200, "https://plus.espn.com/", &[], "ok")),
        parse::no_region()
    );
    assert_eq!(
        espn_plus::parse(&response(
            302,
            "https://plus.espn.com/",
            &[("location", "/denied")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        sling_tv::parse(&response(200, "https://www.sling.com/", &[], "ok")),
        Verdict::yes_in("US")
    );
    assert_eq!(
        pluto_tv::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"session":{"countryCode":"US"}}"#
        )),
        Verdict::yes_in("US")
    );
    assert_eq!(
        crave::parse(&response(200, "https://x", &[], "{}")),
        parse::no_region()
    );
    assert_eq!(
        crave::parse(&response(403, "https://x", &[], "GeoRestriction")),
        Verdict::no()
    );
    assert_eq!(
        cbc_gem::parse(&response(200, "https://x", &[], r#"{"errorCode":0}"#)),
        parse::no_region()
    );
    assert_eq!(
        cbc_gem::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"message":"This content is outside Canada"}"#
        )),
        Verdict::no()
    );

    // Oceania (8)
    assert_eq!(
        stan::parse(&response(
            400,
            "https://x",
            &[],
            r#"{"errors":["Invalid"]}"#
        )),
        parse::no_region()
    );
    assert_eq!(
        stan::parse(&response(
            403,
            "https://x",
            &[],
            r#"{"errors":["StreamCo.Concurrency.GeoBlocked"]}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        binge::parse(&response(401, "https://x", &[], "{}")),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        kayo_sports::parse(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        nine_now::parse(&response(200, "https://x", &[], "{}")),
        parse::no_region()
    );
    assert_eq!(
        nine_now::parse(&response(200, "https://x", &[], r#"{"geoblocked":true}"#)),
        Verdict::no()
    );
    assert_eq!(
        seven_plus::parse(&response(200, "https://x", &[], r#"{"token":"abc"}"#)),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        abc_iview::parse(&response(200, "https://x", &[], "{}")),
        parse::no_region()
    );
    assert_eq!(
        abc_iview::parse(&response(403, "https://x", &[], "outside australia")),
        Verdict::no()
    );
    assert_eq!(
        tvnz_plus::parse(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("NZ")
    );
    assert_eq!(
        threenow::parse(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("NZ")
    );
}
