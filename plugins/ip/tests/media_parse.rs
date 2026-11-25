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

    let akamai_geo_block = response(
        403,
        "https://ssl.zdf.de/geo/de/geo.txt",
        &[],
        "<HTML><HEAD><TITLE>Access Denied</TITLE></HEAD><BODY>errors.edgesuite.net</BODY></HTML>",
    );
    assert_eq!(parse::gate(&akamai_geo_block), None);

    let akamai_bot_challenge = response(
        403,
        "https://www.example.com/",
        &[],
        "<html><body><iframe id=\"sec-cpt-if\" src=\"/_sec/cp_challenge\"></iframe></body></html>",
    );
    assert_eq!(
        parse::gate(&akamai_bot_challenge),
        Some(Verdict::BlockedWaf)
    );

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
    assert_eq!(
        parse::url_country("https://tv.apple.com/au?l=en"),
        Some("AU".to_owned())
    );
    assert_eq!(
        parse::url_country("//www.max.com/gb-en?utm_source=x"),
        Some("GB".to_owned())
    );
    assert_eq!(
        parse::url_country("https://www.hbomax.com/login?returnUrl=/us/home"),
        None
    );
    let cookie_res = response(
        200,
        "https://www.max.com/",
        &[
            ("set-cookie", "countryCode=; Max-Age=0; domain=.max.com"),
            (
                "set-cookie",
                "countryCode=\"EU\"; path=/; domain=.hbomax.com",
            ),
            ("set-cookie", "other=1; countryCode=US"),
        ],
        "",
    );
    assert_eq!(
        parse::cookie(&cookie_res, "countryCode").as_deref(),
        Some("EU")
    );
    assert_eq!(
        parse::cookies_header(&cookie_res).as_deref(),
        Some("countryCode=\"EU\"; other=1")
    );
    assert_eq!(
        parse::base64_decode("eyJtYXJrZXQiOiJISyJ9").as_deref(),
        Some(br#"{"market":"HK"}"#.as_slice())
    );
    assert_eq!(
        parse::jwt_payload("eyJhbGciOiJub25lIn0.eyJjb3VudHJ5IjoiSEsifQ.sig")
            .and_then(|v| v
                .get("country")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned))
            .as_deref(),
        Some("HK")
    );
    assert_eq!(
        parse::cf_trace_loc("fl=1\nloc=tw\ncolo=TPE\n").as_deref(),
        Some("TW")
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
    assert_eq!(
        netflix::parse(&nf_us, &nf_proxy),
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
    let disney_bcp47 = response(
        302,
        "https://www.disneyplus.com/",
        &[("location", "https://www.disneyplus.com/fr-ca?cid=1")],
        "",
    );
    assert_eq!(disney::parse(&disney_bcp47), Verdict::yes_in("CA"));
    let disney_unsup = response(
        200,
        "https://www.disneyplus.com/",
        &[],
        r#"{"countryCode":"CN","inSupportedLocation":false}"#,
    );
    assert_eq!(disney::parse(&disney_unsup), Verdict::no());

    let pv = response(
        200,
        "https://www.primevideo.com/",
        &[],
        r#"<script>{"currentTerritory":"GB"}</script>"#,
    );
    assert_eq!(prime_video::parse(&pv), Verdict::yes_in("GB"));

    let atv_hk = response(200, "https://gspe1-ssl.ls.apple.com/pep/gcc", &[], "HK\n");
    assert_eq!(apple_tv::parse(&atv_hk), Verdict::yes_in("HK"));
    let atv_cn = response(200, "https://gspe1-ssl.ls.apple.com/pep/gcc", &[], "CN");
    assert_eq!(apple_tv::parse(&atv_cn), Verdict::no());
    let atv_url = response(200, "https://tv.apple.com/au", &[], "");
    assert_eq!(apple_tv::parse(&atv_url), Verdict::yes_in("AU"));

    let tt_trace_tw = response(
        200,
        "https://www.tiktok.com/cdn-cgi/trace",
        &[],
        "fl=1\nloc=TW\n",
    );
    let tt_hk_redir = response(200, "https://www.tiktok.com/hk/about", &[], "<html></html>");
    let tt_hk = response(
        200,
        "https://www.tiktok.com/explore",
        &[],
        concat!(
            r#"{"__DEFAULT_SCOPE__":{"webapp.app-context":"#,
            r#"{"language":"en","region":"HK","appId":1233}}}"#
        ),
    );
    assert_eq!(tiktok::parse(&tt_trace_tw), Verdict::yes_in("TW"));
    assert_eq!(tiktok::parse(&tt_hk_redir), Verdict::no());
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
        "https://open.spotify.com/",
        &[],
        r#"<script id="appServerConfig" type="text/plain">eyJtYXJrZXQiOiJISyJ9</script>"#,
    );
    assert_eq!(spotify::parse(&sp), Verdict::yes_in("HK"));
    let sp_cn = response(
        200,
        "https://open.spotify.com/",
        &[],
        r#"<script id="appServerConfig" type="text/plain">eyJtYXJrZXQiOiJDTiJ9</script>"#,
    );
    assert_eq!(spotify::parse(&sp_cn), Verdict::no());

    let dazn_res = response(
        200,
        "https://startup.core.indazn.com/misl/v5/Startup",
        &[],
        r#"{"Region":{"isAllowed":true,"GeolocatedCountry":"de"}}"#,
    );
    assert_eq!(dazn::parse(&dazn_res), Verdict::yes_in("DE"));
    let dazn_odata = response(
        400,
        "https://startup.core.indazn.com/misl/v5/Startup",
        &[],
        r#"{"odata.error":{"code":10000,"message":{"value":"NotAvailableInCountry"}}}"#,
    );
    assert_eq!(dazn::parse(&dazn_odata), Verdict::no());
    assert_eq!(
        dazn::parse(&response(
            403,
            "https://startup.core.indazn.com/misl/v5/Startup",
            &[],
            r#"{"odata.error":{"code":"10006","message":{"value":"VPN or proxy detected"}}}"#,
        )),
        Verdict::No {
            restriction: Some(Restriction::ProxyDetected)
        }
    );

    let cr_jwt = response(
        200,
        "https://beta-api.crunchyroll.com/auth/v1/token",
        &[],
        r#"{"access_token":"eyJhbGciOiJub25lIn0.eyJjb3VudHJ5IjoiSEsifQ.sig"}"#,
    );
    assert_eq!(crunchyroll::parse(&cr_jwt), Verdict::yes_in("HK"));
    let cr_oauth_err = response(
        401,
        "https://beta-api.crunchyroll.com/auth/v1/token",
        &[],
        r#"{"error":"invalid_client"}"#,
    );
    assert_eq!(
        crunchyroll::parse(&cr_oauth_err),
        Verdict::Unexpected("OAuth error: invalid_client".to_owned())
    );

    let tw = response(
        200,
        "https://gql.twitch.tv/gql",
        &[],
        r#"{"data":{"requestInfo":{"countryCode":"SK"}}}"#,
    );
    assert_eq!(twitch::parse(&tw), Verdict::yes_in("SK"));
    assert_eq!(
        twitch::parse(&response(
            200,
            "https://gql.twitch.tv/gql",
            &[],
            r#"{"data":null,"errors":[{"message":"PersistedQueryNotFound"}]}"#,
        )),
        Verdict::Unexpected("Twitch GraphQL error".to_owned())
    );

    let rk = response(
        200,
        "https://gizmo.rakuten.tv/v3/me/start",
        &[],
        r#"{"errors":[],"data":{"iso3166_code":"es"}}"#,
    );
    assert_eq!(rakuten_tv::parse(&rk), Verdict::yes_in("ES"));

    let mubi_redir = response(200, "https://mubi.com/en/hk", &[], "<html></html>");
    assert_eq!(mubi::parse(&mubi_redir), Verdict::yes_in("HK"));
    assert_eq!(
        mubi::parse(&response(
            200,
            "https://mubi.com/en/cn",
            &[],
            "<html></html>"
        )),
        Verdict::no()
    );
    assert_eq!(
        mubi::parse(&response(401, "https://x", &[], r#"{"code":"UA"}"#)),
        Verdict::Unexpected("HTTP 401".to_owned())
    );
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
    let bili_412 = response(412, "https://api.bilibili.com/x", &[], r#"{"code":-412}"#);
    let bili_err = response(200, "https://api.bilibili.com/x", &[], r#"{"code":-404}"#);
    assert_eq!(bilibili::parse(&bili_no, &bili_412), Verdict::BlockedWaf);
    assert_eq!(bilibili::parse(&bili_412, &bili_ok), Verdict::BlockedWaf);
    assert_eq!(
        bilibili::parse(&bili_no, &bili_err),
        Verdict::Unexpected("HTTP 200/200".to_owned())
    );

    let baha_dev = response(
        200,
        "https://ani.gamer.com.tw/ajax/getdeviceid.php",
        &[],
        r#"{"deviceid":"dev123"}"#,
    );
    assert_eq!(bahamut::device_id(&baha_dev).as_deref(), Some("dev123"));
    assert_eq!(
        bahamut::device_id(&response(
            200,
            "https://ani.gamer.com.tw/ajax/getdeviceid.php",
            &[],
            r#"{"deviceid":"bad&sn=1"}"#
        )),
        None
    );
    let baha_tok = response(
        200,
        "https://ani.gamer.com.tw/ajax/token.php",
        &[],
        r#"{"animeSn":37783}"#,
    );
    assert_eq!(bahamut::parse(&baha_tok), Verdict::yes_in("TW"));
    assert_eq!(
        bahamut::parse(&response(
            200,
            "https://ani.gamer.com.tw/ajax/token.php",
            &[],
            r#"{"animeSn":"0","error":{"code":1004,"message":"sn not found"}}"#,
        )),
        Verdict::Unexpected("Bahamut error code 1004".to_owned())
    );
    assert_eq!(
        bahamut::parse(&response(404, "https://x", &[], r#"{"error":"Not Found"}"#)),
        Verdict::Unexpected("HTTP 404".to_owned())
    );

    let viu_sg = response(200, "https://www.viu.com/ott/sg/en-us", &[], "");
    let viu_no = response(200, "https://www.viu.com/ott/no-service/", &[], "");
    assert_eq!(viu::parse(&viu_sg), Verdict::yes_in("SG"));
    assert_eq!(viu::parse(&viu_no), Verdict::no());

    assert_eq!(
        mytvsuper::parse(&response(200, "https://x", &[], r#"{"region":1}"#)),
        Verdict::yes_in("HK")
    );
    assert_eq!(
        hoy_tv::parse(&response(
            200,
            "https://hoytv-live-stream.hoy.tv/ch78/index-fhd.m3u8",
            &[],
            "#EXTM3U\n#EXT-X-VERSION:3\n"
        )),
        Verdict::yes_in("HK")
    );
    assert_eq!(
        hoy_tv::parse(&response(200, "https://x", &[], r#"{"code": 403}"#)),
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
        kktv::parse(&response(401, "https://x", &[], r#"{"country":"TW"}"#)),
        Verdict::Unexpected("HTTP 401".to_owned())
    );
    assert_eq!(
        litv::parse(&response(200, "https://x", &[], r#"{"errorMessage":null}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        litv::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"jsonrpc":"2.0","error":{"code":42000087,"message":"OutsideRegionError: "}}"#
        )),
        Verdict::no()
    );
    // AES-256-CTR decrypt_slur roundtrip against known key = SHA-256(sig || nonce).
    assert_eq!(litv::decrypt_slur("001122", "sig", "nonce"), None);
    let litv_plain = litv::decrypt_slur(
        concat!(
            "000102030405060708090a0b",
            "00000000000000000000000000000000",
            "d42407107269aa656c781374d8e47cc72dc73619c4"
        ),
        "testsig",
        "testnonce",
    )
    .expect("32+ byte slur hex decrypts");
    assert_eq!(litv_plain.len(), 21);
    assert_eq!(
        myvideo::parse(&response(
            200,
            "https://www.myvideo.net.tw/goLoginPage.do",
            &[],
            "myVideo login"
        )),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        myvideo::parse(&response(
            200,
            "https://www.myvideo.net.tw/serviceAreaBlock.do",
            &[],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        linetv::parse(&response(
            200,
            "https://www.linetv.tw/api/geo",
            &[],
            r#"{"ip":"203.0.113.1","country":"TW"}"#
        )),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        linetv::parse(&response(
            200,
            "https://www.linetv.tw/api/geo",
            &[],
            r#"{"ip":"203.0.113.2","country":"HK"}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        friday_video::parse(&response(200, "https://x", &[], r#"{"code":"0000"}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        friday_video::parse(&response(
            200,
            "https://x",
            &[],
            "{\"code\":\"1006\",\"message\":\"\u{975e}\u{53f0}\u{7063}IP\"}"
        )),
        Verdict::no()
    );
    assert_eq!(
        catchplay::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"code":"0","data":{"isoCode":"TW"}}"#
        )),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        catchplay::parse(&response(
            400,
            "https://x",
            &[],
            r#"{"code":"100016","message":"The location HK is not allowed."}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        catchplay::parse(&response(401, "https://x", &[], r#"{"code":"UA"}"#)),
        Verdict::Unexpected("HTTP 401".to_owned())
    );
    assert_eq!(
        hamivideo::parse(&response(200, "https://x", &[], r#"{"code":"06001-107"}"#)),
        Verdict::yes_in("TW")
    );
    assert_eq!(
        hamivideo::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"code":"99999","url":null}"#
        )),
        Verdict::no()
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
        dmm::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"data":{"client":{"isForeignAccess":false}}}"#
        )),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        dmm::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"data":{"client":{"isForeignAccess":true}}}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        dmm::parse(&response(200, "https://x", &[], "{}")),
        Verdict::Unexpected("unrecognized DMM payload".to_owned())
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
            r#"{"result":{"title":"Forbidden Love"}}"#
        )),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        tver::credentials(&response(
            200,
            "https://x",
            &[],
            r#"{"result":{"platform_uid":"u1&x=1","platform_token":"t1"}}"#
        )),
        None
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
            400,
            "https://x",
            &[],
            r#"{"meta":{"status":400,"errorCode":"FORBIDDEN"},"data":{"reasonCode":"MEMBER_ONLY"}}"#
        )),
        Verdict::Unexpected("watch API status 400".to_owned())
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
            r#"{"data":{"webfront_playlistUrl":{"resultStatus":475}}}"#
        )),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        u_next::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"data":{"webfront_playlistUrl":{"resultStatus":467}}}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        lemino::parse(&response(400, "https://x", &[], "{}")),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        nhk_plus::parse(&response(200, "https://x", &[], r#"{"country_code":"JP"}"#)),
        Verdict::yes_in("JP")
    );
    assert_eq!(
        nhk_plus::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"country_code":"other"}"#
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
        Verdict::no()
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
        Verdict::no()
    );
    assert_eq!(
        coupang_play::parse(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("KR")
    );
    assert_eq!(
        coupang_play::parse(&response(
            302,
            "https://x",
            &[("location", "/Not-Available")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        coupang_play::parse(&response(404, "https://x", &[("location", "/home")], "")),
        Verdict::Unexpected("HTTP 404".to_owned())
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
        bbc_iplayer::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"media":[{"href":"https://cdn.example/vpn123"}]}"#
        )),
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
        bbc_iplayer::parse(&response(200, "https://x", &[], r#"{"result":"VPN"}"#)),
        Verdict::No {
            restriction: Some(Restriction::ProxyDetected)
        }
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
        sky_go::parse(&response(
            200,
            "https://x",
            &[],
            "<div id=\"skygoSignin\"></div>"
        )),
        Verdict::yes_in("GB")
    );
    assert_eq!(
        sky_go::parse(&response(403, "https://x", &[], "geoblocked")),
        Verdict::no()
    );
    assert_eq!(
        zdf::parse(&response(404, "https://x", &[], "Not Found")),
        Verdict::yes_in("DE")
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
        joyn::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"entitlement_token":"tok123"}"#
        )),
        Verdict::yes_in("DE")
    );
    assert_eq!(
        joyn::parse(&response(
            400,
            "https://x",
            &[],
            r#"[{"code":"ENT_AssetNotAvailableInCountry"}]"#
        )),
        Verdict::no()
    );
    assert_eq!(
        rtl_plus::parse(&response(
            200,
            "https://front-auth.rtlde.bedrock.tech/v2/rtlde/platforms/m6group_web/token",
            &[],
            r#"{"token":"eyJhbGciOiJub25lIn0.eyJnZW8iOnsiY291bnRyeV9jb2RlIjoiREUifX0.sig"}"#
        )),
        Verdict::yes_in("DE")
    );
    assert_eq!(
        rtl_plus::parse(&response(
            200,
            "https://front-auth.rtlde.bedrock.tech/v2/rtlde/platforms/m6group_web/token",
            &[],
            r#"{"token":"eyJhbGciOiJub25lIn0.eyJnZW8iOnsiY291bnRyeV9jb2RlIjoiV1cifX0.sig"}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        canal_plus::parse(&response(200, "https://x", &[], "ok")),
        Verdict::yes_in("FR")
    );
    assert_eq!(
        canal_plus::parse(&response(
            302,
            "https://x",
            &[("location", "http://www.canalplus.com/othercountry/")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        molotov::parse(&response(200, "https://x", &[], r#"{"is_france":true}"#)),
        Verdict::yes_in("FR")
    );
    assert_eq!(
        molotov::parse(&response(200, "https://x", &[], r#"{"is_france":false}"#)),
        Verdict::no()
    );
    assert_eq!(
        france_tv::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"reponse":{"geo_info":{"country_code":"FR"}}}"#
        )),
        Verdict::yes_in("FR")
    );
    assert_eq!(
        france_tv::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"reponse":{"geo_info":{"country_code":"HK"}}}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        rai_play::parse(&response(200, "https://x", &[], "<video/>")),
        Verdict::yes_in("IT")
    );
    assert_eq!(
        rai_play::parse(&response(
            302,
            "https://x",
            &[("location", "https://cdn.rai.it/video_no_available.mp4")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        movistar_plus::parse(&response(200, "https://login.movistarplus.es/", &[], "ok")),
        Verdict::yes_in("ES")
    );
    assert_eq!(
        movistar_plus::parse(&response(
            403,
            "https://login.movistarplus.es/",
            &[],
            "Forbidden"
        )),
        Verdict::no()
    );
    assert_eq!(
        movistar_plus::parse(&response(
            200,
            "https://x",
            &[],
            "No disponible en tu pa&iacute;s"
        )),
        Verdict::no()
    );
    assert_eq!(
        nlziet::parse(&response(200, "https://x", &[], "fl=1\nloc=NL\n")),
        Verdict::yes_in("NL")
    );
    assert_eq!(
        nlziet::parse(&response(200, "https://x", &[], "fl=1\nloc=HK\n")),
        Verdict::no()
    );
    assert_eq!(
        viaplay::parse(&response(
            200,
            "https://viaplay.com/se-sv/",
            &[],
            "<html lang=\"sv\"></html>"
        )),
        Verdict::yes_in("SE")
    );
    assert_eq!(
        viaplay::parse(&response(
            200,
            "https://viaplay.com/hk/",
            &[],
            "<html lang=\"en-UNAVAILABILITY\"></html>"
        )),
        Verdict::no()
    );

    // Americas (10)
    assert_eq!(
        hulu::parse(&response(
            400,
            "https://auth.hulu.com/v4/web/password/authenticate",
            &[],
            r#"{"error":{"name":"LOGIN_BAD_REQUEST"}}"#
        )),
        Verdict::yes_in("US")
    );
    assert_eq!(
        hulu::parse(&response(
            403,
            "https://www.hulu.com/",
            &[],
            "Anonymous Proxy Tool detected"
        )),
        Verdict::No {
            restriction: Some(Restriction::ProxyDetected)
        }
    );
    assert_eq!(
        hulu::parse(&response(
            302,
            "https://www.hulu.com/",
            &[("location", "https://www.disneyplus.com/")],
            ""
        )),
        Verdict::no()
    );
    assert_eq!(
        hulu::parse(&response(
            302,
            "https://www.hulu.com/",
            &[("location", "https://www.hulu.jp/")],
            ""
        )),
        Verdict::yes_in("JP")
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
        max::parse(&response(
            404,
            "https://www.max.com/",
            &[("x-wbd-country", "US")],
            ""
        )),
        Verdict::Unexpected("HTTP 404".to_owned())
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
            406,
            "https://www.paramountplus.com/",
            &[],
            "406 Not Acceptable"
        )),
        Verdict::no()
    );
    assert_eq!(
        paramount_plus::parse(&response(
            200,
            "https://www.paramountplus.com/ca/",
            &[],
            "ok"
        )),
        Verdict::yes_in("CA")
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
        espn_plus::parse(&response(
            200,
            "https://espn.api.edge.bamgrid.com/graph/v1/device/graphql",
            &[],
            concat!(
                r#"{"data":{"refreshToken":{"activeSession":{"#,
                r#""inSupportedLocation":true,"location":{"countryCode":"US"}}}}}"#
            )
        )),
        Verdict::yes_in("US")
    );
    assert_eq!(
        espn_plus::parse(&response(
            200,
            "https://espn.api.edge.bamgrid.com/graph/v1/device/graphql",
            &[],
            concat!(
                r#"{"data":{"refreshToken":{"activeSession":{"#,
                r#""inSupportedLocation":true,"location":{"countryCode":"HK"}}}}}"#
            )
        )),
        Verdict::no()
    );
    assert_eq!(
        sling_tv::parse(&response(
            200,
            "https://p-geo.movetv.com/geo",
            &[],
            r#"{"playback_allowed":true,"playback_disallowed":false}"#
        )),
        Verdict::yes_in("US")
    );
    assert_eq!(
        sling_tv::parse(&response(
            200,
            "https://p-geo.movetv.com/geo",
            &[],
            r#"{"playback_allowed":null,"playback_disallowed":true}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        pluto_tv::parse(&response(200, "https://pluto.tv/us/live-tv", &[], "ok")),
        Verdict::yes_in("US")
    );
    assert_eq!(
        pluto_tv::parse(&response(200, "https://pluto.tv/intl/", &[], "ok")),
        Verdict::no()
    );
    assert_eq!(
        crave::parse(&response(
            200,
            "https://x",
            &[],
            "<MPD><BaseURL>https://video.9c9media.com/</BaseURL></MPD>"
        )),
        Verdict::yes_in("CA")
    );
    assert_eq!(
        crave::parse(&response(
            403,
            "https://x",
            &[],
            r#"{"ErrorCode":"403.72","Message":"Access denied. Geo Constraint Restrictions"}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        cbc_gem::parse(&response(
            200,
            "https://www.cbc.ca/g/stats/js/cbc-stats-top.js",
            &[],
            r#"CBC.APP.SC.LOCALGEO={"geo":{"country":"CA"}}"#
        )),
        Verdict::yes_in("CA")
    );
    assert_eq!(
        cbc_gem::parse(&response(
            200,
            "https://www.cbc.ca/g/stats/js/cbc-stats-top.js",
            &[],
            r#"CBC.APP.SC.LOCALGEO={"geo":{"country":"HK"}}"#
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
        Verdict::yes_in("AU")
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
        nine_now::parse(&response(
            200,
            "https://x",
            &[],
            "<title>Log in to 9Now</title>"
        )),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        nine_now::parse(&response(403, "https://x", &[], "Geoblock")),
        Verdict::no()
    );
    assert_eq!(
        seven_plus::parse(&response(200, "https://x", &[], r#"{"_id":4}"#)),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        seven_plus::parse(&response(
            404,
            "https://x",
            &[],
            r#"{"code":"Forbidden","message":"Australia only","api-error-code":403001}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        abc_iview::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"_embedded":{"highlightVideo":{"playable":true}}}"#
        )),
        Verdict::yes_in("AU")
    );
    assert_eq!(
        abc_iview::parse(&response(
            200,
            "https://x",
            &[],
            r#"{"_embedded":{"highlightVideo":{"playable":false}}}"#
        )),
        Verdict::no()
    );
    assert_eq!(
        tvnz_plus::parse(&response(200, "https://x", &[], "{}")),
        Verdict::yes_in("NZ")
    );
    assert_eq!(
        threenow::parse(&response(200, "https://x", &[], "#EXTM3U\n")),
        Verdict::yes_in("NZ")
    );
    assert_eq!(
        threenow::parse(&response(403, "https://x", &[], "Request blocked.")),
        Verdict::no()
    );
}
