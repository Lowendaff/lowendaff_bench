//! Bounded-concurrency probe orchestration across IP families and geographic groups.

use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;
use bench_api::Step;
use bench_api::Stop;

use super::parse;
use super::parse::americas;
use super::parse::asia;
use super::parse::europe;
use super::parse::global;
use super::parse::oceania;
use super::result::Failure;
use super::result::Group;
use super::result::Output;
use super::result::ServiceResult;
use super::result::Settings;
use super::result::Verdict;
use super::services::ProbeKind;
use super::services::ServiceSpec;

/// Concurrent service probes active at once within a regional group.
const CONCURRENCY: usize = 10;

/// Runs every selected service over IPv4 and IPv6 (where reachable), publishing incremental
/// results after each regional group completes.
pub async fn measure(
    ctx: &Context<Output>,
    specs: &[ServiceSpec],
    unknown: Vec<String>,
    settings: Settings,
    base_url: Option<&str>,
) -> Result<Output, Stop<Failure>> {
    let network = &ctx.host().network;
    let ipv4_reachable = network.ipv4.reachable();
    let ipv6_reachable = network.ipv6.reachable();
    if !ipv4_reachable && !ipv6_reachable {
        return Err(Stop::Skipped(Failure::NoFamily));
    }

    let results: Vec<ServiceResult> = specs
        .iter()
        .map(|spec| ServiceResult {
            id: spec.id,
            name: spec.name,
            group: spec.group,
            ipv4: None,
            ipv6: None,
        })
        .collect();
    let mut output = Output {
        settings,
        unknown,
        ipv4_reachable,
        ipv6_reachable,
        results,
    };
    ctx.publish(&output);

    let families = [
        (IpFamily::Ipv4, ipv4_reachable, false),
        (IpFamily::Ipv6, ipv6_reachable, true),
    ];
    for (family, reachable, ipv6) in families {
        if !reachable {
            continue;
        }
        for group in Group::ALL {
            let indices: Vec<(usize, ServiceSpec)> = specs
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, spec)| spec.group == group)
                .collect();
            if indices.is_empty() {
                continue;
            }
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            ctx.progress(Step::Media {
                group: group.code(),
                ipv6,
            });
            let tasks = indices.iter().copied().map(|(idx, spec)| async move {
                (idx, probe_one(ctx, family, spec, base_url).await)
            });
            let verdicts = run_bounded(CONCURRENCY, tasks).await;
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            for (idx, verdict) in verdicts {
                if ipv6 {
                    output.results[idx].ipv6 = Some(verdict);
                } else {
                    output.results[idx].ipv4 = Some(verdict);
                }
            }
            ctx.publish(&output);
        }
    }

    Ok(output)
}

type BoxedTask<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Executes `tasks` with at most `limit` in flight at once, returning results in input order.
async fn run_bounded<'a, T, I, Fut>(limit: usize, tasks: I) -> Vec<T>
where
    I: IntoIterator<Item = Fut>,
    Fut: Future<Output = T> + Send + 'a,
{
    let cap = limit.max(1);
    let mut pending = tasks.into_iter().enumerate();
    let mut active: Vec<(usize, BoxedTask<'a, T>)> = Vec::new();
    let mut done: Vec<(usize, T)> = Vec::new();
    loop {
        while active.len() < cap {
            let Some((idx, fut)) = pending.next() else {
                break;
            };
            active.push((idx, Box::pin(fut)));
        }
        if active.is_empty() {
            break;
        }
        let (pos, value) = std::future::poll_fn(|cx| {
            for (index, (_, fut)) in active.iter_mut().enumerate() {
                if let Poll::Ready(value) = fut.as_mut().poll(cx) {
                    return Poll::Ready((index, value));
                }
            }
            Poll::Pending
        })
        .await;
        let (orig_idx, _) = active.swap_remove(pos);
        done.push((orig_idx, value));
    }
    done.sort_by_key(|(idx, _)| *idx);
    done.into_iter().map(|(_, value)| value).collect()
}

fn endpoint(base_url: Option<&str>, id: &str, step: &str, real_url: &str) -> String {
    match base_url {
        Some(base) => format!("{base}/{id}/{step}"),
        None => real_url.to_owned(),
    }
}

async fn send(
    ctx: &Context<Output>,
    request: HttpRequest,
) -> Result<bench_api::HttpResponse, Verdict> {
    ctx.probe(&request)
        .await
        .map_err(|err| parse::error_verdict(&err))
}

/// Probes one streaming service over `family`.
pub async fn probe_one(
    ctx: &Context<Output>,
    family: IpFamily,
    spec: ServiceSpec,
    base_url: Option<&str>,
) -> Verdict {
    let url = |step: &str, real: &str| endpoint(base_url, spec.id, step, real);
    match spec.kind {
        // --- Multi-request / two-step probes ---
        ProbeKind::Netflix => {
            let licensed_req = HttpRequest::get(
                family,
                url("licensed", "https://www.netflix.com/title/81280792"),
            )
            .with_redirect(RedirectPolicy::Follow { max: 5 });
            let original_req = HttpRequest::get(
                family,
                url("original", "https://www.netflix.com/title/80018499"),
            )
            .with_redirect(RedirectPolicy::Follow { max: 5 });
            let (licensed, original) =
                tokio::join!(ctx.probe(&licensed_req), ctx.probe(&original_req));
            match (licensed, original) {
                (Ok(lic), Ok(orig)) => global::netflix(&lic, &orig),
                (Err(err), _) | (_, Err(err)) => parse::error_verdict(&err),
            }
        }
        ProbeKind::Disney => {
            let web_req = HttpRequest::get(family, url("web", "https://www.disneyplus.com/"));
            let web = match send(ctx, web_req).await {
                Ok(res) => res,
                Err(verdict) => return verdict,
            };
            if let Some(early) = global::disney_web_early(&web) {
                return early;
            }
            let gql_payload = br#"{"query":"query { __typename }"}"#.to_vec();
            let api_req = HttpRequest::post(
                family,
                url(
                    "api",
                    "https://disney.api.edge.bamgrid.com/v1/public/graphql",
                ),
                gql_payload,
            )
            .with_header("content-type", "application/json");
            let api = ctx.probe(&api_req).await.ok();
            global::disney(&web, api.as_ref())
        }
        ProbeKind::Bilibili => {
            let tw_req = HttpRequest::get(
                family,
                url(
                    "tw",
                    "https://api.bilibili.com/pgc/player/web/playurl?ep_id=507946&qn=80&fnval=16",
                ),
            );
            let hmt_req = HttpRequest::get(
                family,
                url(
                    "hmt",
                    "https://api.bilibili.com/pgc/player/web/playurl?ep_id=285145&qn=80&fnval=16",
                ),
            );
            let (tw, hmt) = tokio::join!(ctx.probe(&tw_req), ctx.probe(&hmt_req));
            match (tw, hmt) {
                (Ok(tw_res), Ok(hmt_res)) => asia::bilibili(&tw_res, &hmt_res),
                (Err(err), _) | (_, Err(err)) => parse::error_verdict(&err),
            }
        }
        ProbeKind::Bahamut => {
            let dev_req = HttpRequest::get(
                family,
                url("device", "https://ani.gamer.com.tw/ajax/getdeviceid.php"),
            );
            let dev_res = match send(ctx, dev_req).await {
                Ok(res) => res,
                Err(verdict) => return verdict,
            };
            let Some(device_id) = asia::bahamut_device_id(&dev_res) else {
                return asia::bahamut_device_failure(&dev_res);
            };
            let token_url = match base_url {
                Some(base) => format!("{base}/bahamut/token?device={device_id}"),
                None => format!(
                    "https://ani.gamer.com.tw/ajax/token.php?adID=89422&sn=37783&device={device_id}"
                ),
            };
            let tok_req = HttpRequest::get(family, token_url).with_header(
                "referer",
                "https://ani.gamer.com.tw/animeVideo.php?sn=37783",
            );
            match send(ctx, tok_req).await {
                Ok(tok_res) => asia::bahamut_token(&tok_res),
                Err(verdict) => verdict,
            }
        }
        ProbeKind::Tver => {
            let create_req = HttpRequest::post(
                family,
                url(
                    "create",
                    "https://platform-api.tver.jp/v2/api/platform_users/browser/create",
                ),
                b"device_type=pc".to_vec(),
            )
            .with_header("content-type", "application/x-www-form-urlencoded")
            .with_header("origin", "https://s.tver.jp")
            .with_header("referer", "https://s.tver.jp/");
            let create_res = match send(ctx, create_req).await {
                Ok(res) => res,
                Err(verdict) => return verdict,
            };
            let Some((uid, token)) = asia::tver_credentials(&create_res) else {
                return asia::tver_create_verdict(&create_res);
            };
            let check_url = match base_url {
                Some(base) => format!("{base}/tver/callapi"),
                None => format!(
                    "https://platform-api.tver.jp/service/api/v1/callHome\
                     ?platform_uid={uid}&platform_token={token}&require_data=mylist"
                ),
            };
            let check_req =
                HttpRequest::get(family, check_url).with_header("x-tver-platform-type", "web");
            match send(ctx, check_req).await {
                Ok(res) => asia::tver_home(&res),
                Err(verdict) => verdict,
            }
        }

        // --- Single-request probes ---
        other => {
            let req = single_request(family, other, spec.id, base_url);
            match send(ctx, req).await {
                Ok(res) => parse_single(other, &res),
                Err(verdict) => verdict,
            }
        }
    }
}

fn single_request(
    family: IpFamily,
    kind: ProbeKind,
    id: &str,
    base_url: Option<&str>,
) -> HttpRequest {
    let url = |real: &str| endpoint(base_url, id, "check", real);
    let follow = RedirectPolicy::Follow { max: 5 };
    match kind {
        // Global
        ProbeKind::YouTube => HttpRequest::get(family, url("https://www.youtube.com/premium"))
            .with_header("cookie", "CONSENT=YES+cb.20210328-17-p0.en+FX+700")
            .with_redirect(follow),
        ProbeKind::PrimeVideo => {
            HttpRequest::get(family, url("https://www.primevideo.com/")).with_redirect(follow)
        }
        ProbeKind::AppleTv => {
            HttpRequest::get(family, url("https://tv.apple.com/")).with_redirect(follow)
        }
        ProbeKind::TikTok => HttpRequest::get(family, url("https://www.tiktok.com/cdn-cgi/trace")),
        ProbeKind::Spotify => HttpRequest::post(
            family,
            url("https://spclient.wg.spotify.com/signup/public/v1/account"),
            concat!(
                "birth_day=1&birth_month=1&birth_year=1995&collect_personal_info=undefined",
                "&creation_flow=&creation_point=https%3A%2F%2Fwww.spotify.com%2F",
                "&displayname=bench&gender=neutral&iagree=1",
                "&key=a1e486e2729f46d6bb368d6b2bcda326&platform=www",
                "&referrer=&send-email=0&thirdpartyemail=0"
            )
            .as_bytes()
            .to_vec(),
        )
        .with_header("content-type", "application/x-www-form-urlencoded"),
        ProbeKind::Dazn => HttpRequest::post(
            family,
            url("https://startup.core.indazn.com/misl/v5/Startup"),
            concat!(
                r#"{"LandingPageKey":"generic","Languages":"en-US,en","#,
                r#""Platform":"web","Manufacturer":"","PromoCode":""}"#
            )
            .as_bytes()
            .to_vec(),
        )
        .with_header("content-type", "application/json"),
        ProbeKind::Crunchyroll => HttpRequest::post(
            family,
            url("https://www.crunchyroll.com/auth/v1/token"),
            b"grant_type=client_id".to_vec(),
        )
        .with_header("content-type", "application/x-www-form-urlencoded")
        .with_header("authorization", "Basic bm9haWhkZXZtXzZpeWcwYThsMHE6"),
        ProbeKind::Twitch => HttpRequest::post(
            family,
            url("https://gql.twitch.tv/gql"),
            br#"{"query":"query{requestInfo{countryCode}}"}"#.to_vec(),
        )
        .with_header("client-id", "kimne78kx3ncx6brgo4mv6wki5h1ko")
        .with_header("content-type", "application/json"),
        ProbeKind::RakutenTv => HttpRequest::get(
            family,
            url("https://gizmo.rakuten.tv/v3/me/start?classification_id=5&device_identifier=web"),
        ),
        ProbeKind::Mubi => HttpRequest::get(family, url("https://api.mubi.com/v3/country"))
            .with_header("client", "web"),

        // Asia
        ProbeKind::Viu => {
            HttpRequest::get(family, url("https://www.viu.com/")).with_redirect(follow)
        }
        ProbeKind::MyTvSuper => HttpRequest::get(
            family,
            url("https://www.mytvsuper.com/api/auth/getSession/self/"),
        ),
        ProbeKind::HoyTv => HttpRequest::get(family, url("https://api2.hoy.tv/api/v3/a/channel")),
        ProbeKind::Kktv => HttpRequest::get(family, url("https://api.kktv.me/v3/ipcheck")),
        ProbeKind::LiTv => HttpRequest::post(
            family,
            url("https://www.litv.tv/vod/ajax/getUrl"),
            concat!(
                r#"{"type":"noauth","assetId":"vod44868-000001M001_800K","#,
                r#""puid":"00000000-0000-0000-0000-000000000000"}"#
            )
            .as_bytes()
            .to_vec(),
        )
        .with_header("content-type", "application/json"),
        ProbeKind::MyVideo => {
            HttpRequest::get(family, url("https://www.myvideo.net.tw/")).with_redirect(follow)
        }
        ProbeKind::LineTv => HttpRequest::get(
            family,
            url("https://www.linetv.tw/api/part/11829/eps/1/part?chocomemberId="),
        ),
        ProbeKind::FridayVideo => HttpRequest::get(
            family,
            url(concat!(
                "https://video.friday.tw/api2/streaming/get",
                "?streamingId=132287&streamingType=2&contentType=4&contentId=1&clientId="
            )),
        ),
        ProbeKind::Catchplay => HttpRequest::get(family, url("https://sunapi.catchplay.com/geo"))
            .with_header(
                "authorization",
                concat!(
                    "Basic NTQ3MzM0NDgtYTU3Yi00MjU2LWE4MTEt",
                    "MzdlYzNkNjJmM2E0Ok90QzR3elJRR2hLQ01sSDc="
                ),
            ),
        ProbeKind::HamiVideo => HttpRequest::get(
            family,
            url("https://hamivideo.hinet.net/api/play.do?id=OTT_LIVE_0000001844&freeProduct=1"),
        ),
        ProbeKind::Abema => HttpRequest::get(
            family,
            url("https://api.abema.io/v1/ip/check?device=android"),
        ),
        ProbeKind::Dmm => HttpRequest::get(
            family,
            url("https://api.dmm.com/service/digitalapi/-/ip_check/"),
        ),
        ProbeKind::Niconico => {
            HttpRequest::get(family, url("https://www.nicovideo.jp/watch/so23017073"))
                .with_redirect(follow)
        }
        ProbeKind::UNext => HttpRequest::get(
            family,
            url(concat!(
                "https://video-api.unext.jp/api/1/player/url",
                "?title_code=SID0012977&episode_code=ED00067432&play_mode=caption"
            )),
        ),
        ProbeKind::Lemino => HttpRequest::post(
            family,
            url("https://if.lemino.docomo.ne.jp/v1/user/delivery/watch/ready"),
            br#"{"CheckTarget":[{"kind":"1","cid":"0099999999","lid":"0099999999"}]}"#.to_vec(),
        )
        .with_header("content-type", "application/json"),
        ProbeKind::NhkPlus => HttpRequest::get(family, url("https://plus.nhk.jp/api/v1/ipcheck")),
        ProbeKind::Radiko => HttpRequest::get(family, url("https://radiko.jp/area")),
        ProbeKind::Wavve => HttpRequest::get(
            family,
            url(concat!(
                "https://apis.wavve.com/fz/streaming?device=pc&partner=pooq",
                "&apikey=E5F3E0D30947AA5440556471321BB6D9&credential=none&service=wavve",
                "&pooqzone=none&region=kor&drm=wm&targetage=all",
                "&contentid=MV_C901_SG0000113303&contenttype=movie"
            )),
        ),
        ProbeKind::Tving => HttpRequest::get(
            family,
            url(concat!(
                "https://api.tving.com/v2a/media/stream/info",
                "?apiKey=1e7952d0917d6aab1f0293a063697610&mediaCode=RV60891248"
            )),
        ),
        ProbeKind::CoupangPlay => HttpRequest::get(family, url("https://www.coupangplay.com/")),
        ProbeKind::Mewatch => HttpRequest::get(
            family,
            url(concat!(
                "https://cdn.mewatch.sg/api/items/97098/videos",
                "?delivery=stream,progressive&ff=idp,ldp,rpt,cd",
                "&lang=en&resolution=External&segments=all"
            )),
        ),

        // Europe
        ProbeKind::BbcIplayer => HttpRequest::get(
            family,
            url(concat!(
                "https://open.live.bbc.co.uk/mediaselector/6/select/version/2.0",
                "/mediaset/pc/vpid/bbc_one_london/format/json"
            )),
        ),
        ProbeKind::Channel4 => HttpRequest::get(
            family,
            url("https://www.channel4.com/simulcast/channels/C4"),
        ),
        ProbeKind::ItvX => HttpRequest::get(
            family,
            url("https://simulcast.itv.com/playlist/itvonline/ITV"),
        ),
        ProbeKind::SkyGo => HttpRequest::get(
            family,
            url("https://awk.epgsky.com/hawk/linear/services/4097/1"),
        ),
        ProbeKind::Zdf => HttpRequest::get(family, url("https://ssl.zdf.de/geo/de/geo.txt")),
        ProbeKind::Joyn => HttpRequest::get(family, url("https://api.joyn.de/")),
        ProbeKind::RtlPlus => {
            HttpRequest::get(family, url("https://plus.rtl.de/")).with_redirect(follow)
        }
        ProbeKind::CanalPlus => HttpRequest::get(family, url("https://boutique.canalplus.com/")),
        ProbeKind::Molotov => {
            HttpRequest::get(family, url("https://fapi.molotov.tv/v2/startup/config"))
        }
        ProbeKind::FranceTv => {
            HttpRequest::get(family, url("https://k7.ftven.fr/videos/195392964"))
        }
        ProbeKind::RaiPlay => HttpRequest::get(
            family,
            url("https://mediapolisvod.rai.it/relinker/relinkerServlet.htm?cont=2606803&output=64"),
        ),
        ProbeKind::MovistarPlus => {
            HttpRequest::get(family, url("https://contratar.movistarplus.es/"))
        }
        ProbeKind::Nlziet => HttpRequest::get(family, url("https://app.nlziet.nl/")),
        ProbeKind::Viaplay => HttpRequest::get(family, url("https://checkout.viaplay.com/")),

        // Americas
        ProbeKind::Hulu => HttpRequest::get(family, url("https://www.hulu.com/welcome")),
        ProbeKind::Max => HttpRequest::get(family, url("https://www.max.com/")),
        ProbeKind::Peacock => HttpRequest::get(family, url("https://www.peacocktv.com/")),
        ProbeKind::ParamountPlus => HttpRequest::get(family, url("https://www.paramountplus.com/")),
        ProbeKind::Starz => HttpRequest::get(
            family,
            url("https://auth.starz.com/api/v4/User/geolocation"),
        ),
        ProbeKind::EspnPlus => HttpRequest::get(family, url("https://plus.espn.com/")),
        ProbeKind::SlingTv => HttpRequest::get(family, url("https://www.sling.com/")),
        ProbeKind::PlutoTv => HttpRequest::get(
            family,
            url(concat!(
                "https://boot.pluto.tv/v4/start?appName=web&appVersion=7.0.0",
                "&deviceVersion=131.0.0&deviceModel=web&deviceMake=chrome&deviceType=web",
                "&clientID=00000000-0000-4000-8000-000000000000&clientModelNumber=1.0.0"
            )),
        ),
        ProbeKind::Crave => HttpRequest::get(
            family,
            url(concat!(
                "https://capi.9c9media.com/destinations/crave_web",
                "/platforms/desktop/contents/2236485/contentPackages"
            )),
        ),
        ProbeKind::CbcGem => HttpRequest::get(
            family,
            url(concat!(
                "https://services.radio-canada.ca/media/validation/v2/",
                "?appCode=medianetlive&connectionType=hd&deviceType=ipad",
                "&idMedia=cbuft&multibitrate=true&output=json"
            )),
        ),

        // Oceania
        ProbeKind::Stan => HttpRequest::post(
            family,
            url("https://api.stan.com.au/login/v1/sessions/web/account"),
            b"{}".to_vec(),
        )
        .with_header("content-type", "application/json"),
        ProbeKind::Binge => HttpRequest::post(
            family,
            url("https://auth.streamotion.com.au/oauth/token"),
            br#"{"client_id":"check","grant_type":"password"}"#.to_vec(),
        )
        .with_header("content-type", "application/json"),
        ProbeKind::KayoSports => HttpRequest::get(
            family,
            url("https://api.kayosports.com.au/v3/content/types/landing/names/home"),
        ),
        ProbeKind::NineNow => HttpRequest::get(
            family,
            url("https://tv-api.9now.com.au/v2/pages/livestreams?device=web"),
        ),
        ProbeKind::SevenPlus => HttpRequest::get(family, url("https://7plus.com.au/auth/token")),
        ProbeKind::AbcIview => HttpRequest::get(
            family,
            url("https://api.iview.abc.net.au/v2/navigation/mobile"),
        ),
        ProbeKind::TvnzPlus => HttpRequest::get(
            family,
            url("https://apis-public-prod.tech.tvnz.co.nz/api/v1/web/play/page/categories/live"),
        ),
        ProbeKind::ThreeNow => {
            HttpRequest::get(family, url("https://now-api.threenow.co.nz/v3/config/web"))
        }

        // Handled in `probe_one` above.
        ProbeKind::Netflix
        | ProbeKind::Disney
        | ProbeKind::Bilibili
        | ProbeKind::Bahamut
        | ProbeKind::Tver => HttpRequest::get(family, url("https://127.0.0.1/unreachable")),
    }
}

fn parse_single(kind: ProbeKind, response: &bench_api::HttpResponse) -> Verdict {
    match kind {
        ProbeKind::YouTube => global::youtube(response),
        ProbeKind::PrimeVideo => global::prime_video(response),
        ProbeKind::AppleTv => global::apple_tv(response),
        ProbeKind::TikTok => global::tiktok(response),
        ProbeKind::Spotify => global::spotify(response),
        ProbeKind::Dazn => global::dazn(response),
        ProbeKind::Crunchyroll => global::crunchyroll(response),
        ProbeKind::Twitch => global::twitch(response),
        ProbeKind::RakutenTv => global::rakuten_tv(response),
        ProbeKind::Mubi => global::mubi(response),

        ProbeKind::Viu => asia::viu(response),
        ProbeKind::MyTvSuper => asia::mytvsuper(response),
        ProbeKind::HoyTv => asia::hoy_tv(response),
        ProbeKind::Kktv => asia::kktv(response),
        ProbeKind::LiTv => asia::litv(response),
        ProbeKind::MyVideo => asia::myvideo(response),
        ProbeKind::LineTv => asia::linetv(response),
        ProbeKind::FridayVideo => asia::friday_video(response),
        ProbeKind::Catchplay => asia::catchplay(response),
        ProbeKind::HamiVideo => asia::hamivideo(response),
        ProbeKind::Abema => asia::abema(response),
        ProbeKind::Dmm => asia::dmm(response),
        ProbeKind::Niconico => asia::niconico(response),
        ProbeKind::UNext => asia::u_next(response),
        ProbeKind::Lemino => asia::lemino(response),
        ProbeKind::NhkPlus => asia::nhk_plus(response),
        ProbeKind::Radiko => asia::radiko(response),
        ProbeKind::Wavve => asia::wavve(response),
        ProbeKind::Tving => asia::tving(response),
        ProbeKind::CoupangPlay => asia::coupang_play(response),
        ProbeKind::Mewatch => asia::mewatch(response),

        ProbeKind::BbcIplayer => europe::bbc_iplayer(response),
        ProbeKind::Channel4 => europe::channel_4(response),
        ProbeKind::ItvX => europe::itv_x(response),
        ProbeKind::SkyGo => europe::sky_go(response),
        ProbeKind::Zdf => europe::zdf(response),
        ProbeKind::Joyn => europe::joyn(response),
        ProbeKind::RtlPlus => europe::rtl_plus(response),
        ProbeKind::CanalPlus => europe::canal_plus(response),
        ProbeKind::Molotov => europe::molotov(response),
        ProbeKind::FranceTv => europe::france_tv(response),
        ProbeKind::RaiPlay => europe::rai_play(response),
        ProbeKind::MovistarPlus => europe::movistar_plus(response),
        ProbeKind::Nlziet => europe::nlziet(response),
        ProbeKind::Viaplay => europe::viaplay(response),

        ProbeKind::Hulu => americas::hulu(response),
        ProbeKind::Max => americas::max(response),
        ProbeKind::Peacock => americas::peacock(response),
        ProbeKind::ParamountPlus => americas::paramount_plus(response),
        ProbeKind::Starz => americas::starz(response),
        ProbeKind::EspnPlus => americas::espn_plus(response),
        ProbeKind::SlingTv => americas::sling_tv(response),
        ProbeKind::PlutoTv => americas::pluto_tv(response),
        ProbeKind::Crave => americas::crave(response),
        ProbeKind::CbcGem => americas::cbc_gem(response),

        ProbeKind::Stan => oceania::stan(response),
        ProbeKind::Binge => oceania::binge(response),
        ProbeKind::KayoSports => oceania::kayo_sports(response),
        ProbeKind::NineNow => oceania::nine_now(response),
        ProbeKind::SevenPlus => oceania::seven_plus(response),
        ProbeKind::AbcIview => oceania::abc_iview(response),
        ProbeKind::TvnzPlus => oceania::tvnz_plus(response),
        ProbeKind::ThreeNow => oceania::threenow(response),

        ProbeKind::Netflix
        | ProbeKind::Disney
        | ProbeKind::Bilibili
        | ProbeKind::Bahamut
        | ProbeKind::Tver => Verdict::Unexpected("multi-step service routed to single".to_owned()),
    }
}
