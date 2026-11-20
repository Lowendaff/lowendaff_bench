//! Catalog of 68 streaming platforms across Global, Asia, Europe, Americas and Oceania,
//! and selection by group or service id.

use super::result::Choice;
use super::result::Group;

/// How a service is probed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    /// Netflix: non-original and original title checks.
    Netflix,
    /// YouTube Premium: consent cookie and region / Mainland China redirect check.
    YouTube,
    /// Disney+: web root redirect and BAMGrid location check.
    Disney,
    /// Amazon Prime Video: storefront territory check.
    PrimeVideo,
    /// Apple TV+: storefront availability check.
    AppleTv,
    /// TikTok: region trace / privacy config check.
    TikTok,
    /// Spotify: signup account validation check.
    Spotify,
    /// DAZN: `Startup` API geolocation check.
    Dazn,
    /// Crunchyroll: anonymous client token country check.
    Crunchyroll,
    /// Twitch: public GQL availability check.
    Twitch,
    /// Rakuten TV: `gizmo` market check.
    RakutenTv,
    /// MUBI: `/v3/country` check.
    Mubi,
    /// Bilibili: TW-only and HK/MO/TW PGC bangumi `playurl` checks.
    Bilibili,
    /// Bahamut Anime Crazy: two-step `getdeviceid.php` + `token.php` check.
    Bahamut,
    /// Viu: `/ott/<country>/` redirect check.
    Viu,
    /// myTV SUPER: `getSession` Hong Kong region check.
    MyTvSuper,
    /// HOY TV: Hong Kong live channel check.
    HoyTv,
    /// KKTV: `/v3/ipcheck` Taiwan check.
    Kktv,
    /// LiTV: VOD `getUrl` Taiwan check.
    LiTv,
    /// MyVideo: Taiwan `serviceAreaBlock` check.
    MyVideo,
    /// LINE TV: Taiwan episode metadata check.
    LineTv,
    /// friDay Video: Taiwan streaming check.
    FridayVideo,
    /// CATCHPLAY+: `sunapi` country check (`TW`, `SG`, `ID`).
    Catchplay,
    /// Hami Video: Taiwan OTT live play check.
    HamiVideo,
    /// Abema: `/v1/ip/check` Japan check.
    Abema,
    /// DMM TV: Japan foreign IP check.
    Dmm,
    /// TVer: browser token creation and Japan availability check.
    Tver,
    /// Niconico: Japan-restricted watch page check.
    Niconico,
    /// U-NEXT: player URL Japan geo-code check.
    UNext,
    /// Lemino: delivery ready Japan check.
    Lemino,
    /// NHK+: domestic Japan IP check.
    NhkPlus,
    /// radiko: `/area` Japan prefecture check.
    Radiko,
    /// Wavve: Korea streaming API check.
    Wavve,
    /// TVING: Korea media stream info check.
    Tving,
    /// Coupang Play: Korea availability redirect check.
    CoupangPlay,
    /// mewatch: Singapore Mediacorp video delivery check.
    Mewatch,
    /// BBC iPlayer: UK `mediaselector` `bbc_one_london` check.
    BbcIplayer,
    /// Channel 4: UK simulcast check.
    Channel4,
    /// ITVX: UK simulcast playlist check.
    ItvX,
    /// Sky Go: UK/IE linear service check.
    SkyGo,
    /// ZDF: German `geo.txt` check.
    Zdf,
    /// Joyn: German/Austrian availability check.
    Joyn,
    /// RTL+: German streaming check.
    RtlPlus,
    /// Canal+: French storefront check.
    CanalPlus,
    /// Molotov: French `fapi` check.
    Molotov,
    /// france.tv: French video validation check.
    FranceTv,
    /// RaiPlay: Italian `mediapolisvod` relinker check.
    RaiPlay,
    /// Movistar Plus+: Spanish availability check.
    MovistarPlus,
    /// NLZIET: Dutch availability check.
    Nlziet,
    /// Viaplay: Nordic and European country redirect check.
    Viaplay,
    /// Hulu: US welcome availability check.
    Hulu,
    /// Max: `/geo-availability` redirect and country header check.
    Max,
    /// Peacock: `/unavailable` redirect and territory header check.
    Peacock,
    /// Paramount+: `/intl/` redirect and country path check.
    ParamountPlus,
    /// Starz: `/api/v4/User/geolocation` US check.
    Starz,
    /// ESPN+: US availability check.
    EspnPlus,
    /// Sling TV: US availability check.
    SlingTv,
    /// Pluto TV: `boot.pluto.tv` start check.
    PlutoTv,
    /// Crave: Canadian `9c9media` package check.
    Crave,
    /// CBC Gem: Canadian `radio-canada` validation check.
    CbcGem,
    /// Stan: Australian session geo-block check.
    Stan,
    /// Binge: Australian Streamotion auth check.
    Binge,
    /// Kayo Sports: Australian landing API check.
    KayoSports,
    /// 9Now: Australian livestreams API check.
    NineNow,
    /// 7plus: Australian token/market check.
    SevenPlus,
    /// ABC iview: Australian navigation API check.
    AbcIview,
    /// TVNZ+: New Zealand live play API check.
    TvnzPlus,
    /// ThreeNow: New Zealand web config API check.
    ThreeNow,
}

/// Static definition of one streaming service in the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceSpec {
    /// Stable lowercase identifier, such as `bbc-iplayer`.
    pub id: &'static str,
    /// Display name shown in the report.
    pub name: &'static str,
    /// Geographic group.
    pub group: Group,
    /// Probe strategy and parser selector.
    pub kind: ProbeKind,
}

const fn service(
    id: &'static str,
    name: &'static str,
    group: Group,
    kind: ProbeKind,
) -> ServiceSpec {
    ServiceSpec {
        id,
        name,
        group,
        kind,
    }
}

/// All 68 supported streaming platforms, in group and display order.
pub const CATALOG: [ServiceSpec; 68] = [
    // Global (12)
    service("netflix", "Netflix", Group::Global, ProbeKind::Netflix),
    service(
        "youtube",
        "YouTube Premium",
        Group::Global,
        ProbeKind::YouTube,
    ),
    service("disney", "Disney+", Group::Global, ProbeKind::Disney),
    service(
        "prime-video",
        "Prime Video",
        Group::Global,
        ProbeKind::PrimeVideo,
    ),
    service("apple-tv", "Apple TV+", Group::Global, ProbeKind::AppleTv),
    service("tiktok", "TikTok", Group::Global, ProbeKind::TikTok),
    service("spotify", "Spotify", Group::Global, ProbeKind::Spotify),
    service("dazn", "DAZN", Group::Global, ProbeKind::Dazn),
    service(
        "crunchyroll",
        "Crunchyroll",
        Group::Global,
        ProbeKind::Crunchyroll,
    ),
    service("twitch", "Twitch", Group::Global, ProbeKind::Twitch),
    service(
        "rakuten-tv",
        "Rakuten TV",
        Group::Global,
        ProbeKind::RakutenTv,
    ),
    service("mubi", "MUBI", Group::Global, ProbeKind::Mubi),
    // Asia (24)
    service("bilibili", "Bilibili", Group::Asia, ProbeKind::Bilibili),
    service("bahamut", "Bahamut Anime", Group::Asia, ProbeKind::Bahamut),
    service("viu", "Viu", Group::Asia, ProbeKind::Viu),
    service("mytvsuper", "myTV SUPER", Group::Asia, ProbeKind::MyTvSuper),
    service("hoy-tv", "HOY TV", Group::Asia, ProbeKind::HoyTv),
    service("kktv", "KKTV", Group::Asia, ProbeKind::Kktv),
    service("litv", "LiTV", Group::Asia, ProbeKind::LiTv),
    service("myvideo", "MyVideo", Group::Asia, ProbeKind::MyVideo),
    service("linetv", "LINE TV", Group::Asia, ProbeKind::LineTv),
    service(
        "friday",
        "friDay Video",
        Group::Asia,
        ProbeKind::FridayVideo,
    ),
    service("catchplay", "CATCHPLAY+", Group::Asia, ProbeKind::Catchplay),
    service("hamivideo", "Hami Video", Group::Asia, ProbeKind::HamiVideo),
    service("abema", "Abema", Group::Asia, ProbeKind::Abema),
    service("dmm", "DMM TV", Group::Asia, ProbeKind::Dmm),
    service("tver", "TVer", Group::Asia, ProbeKind::Tver),
    service("niconico", "Niconico", Group::Asia, ProbeKind::Niconico),
    service("u-next", "U-NEXT", Group::Asia, ProbeKind::UNext),
    service("lemino", "Lemino", Group::Asia, ProbeKind::Lemino),
    service("nhk-plus", "NHK+", Group::Asia, ProbeKind::NhkPlus),
    service("radiko", "radiko", Group::Asia, ProbeKind::Radiko),
    service("wavve", "Wavve", Group::Asia, ProbeKind::Wavve),
    service("tving", "TVING", Group::Asia, ProbeKind::Tving),
    service(
        "coupang-play",
        "Coupang Play",
        Group::Asia,
        ProbeKind::CoupangPlay,
    ),
    service("mewatch", "mewatch", Group::Asia, ProbeKind::Mewatch),
    // Europe (14)
    service(
        "bbc-iplayer",
        "BBC iPlayer",
        Group::Europe,
        ProbeKind::BbcIplayer,
    ),
    service("channel-4", "Channel 4", Group::Europe, ProbeKind::Channel4),
    service("itv-x", "ITVX", Group::Europe, ProbeKind::ItvX),
    service("sky-go", "Sky Go", Group::Europe, ProbeKind::SkyGo),
    service("zdf", "ZDF", Group::Europe, ProbeKind::Zdf),
    service("joyn", "Joyn", Group::Europe, ProbeKind::Joyn),
    service("rtl-plus", "RTL+", Group::Europe, ProbeKind::RtlPlus),
    service("canal-plus", "Canal+", Group::Europe, ProbeKind::CanalPlus),
    service("molotov", "Molotov", Group::Europe, ProbeKind::Molotov),
    service("france-tv", "france.tv", Group::Europe, ProbeKind::FranceTv),
    service("rai-play", "RaiPlay", Group::Europe, ProbeKind::RaiPlay),
    service(
        "movistar-plus",
        "Movistar Plus+",
        Group::Europe,
        ProbeKind::MovistarPlus,
    ),
    service("nlziet", "NLZIET", Group::Europe, ProbeKind::Nlziet),
    service("viaplay", "Viaplay", Group::Europe, ProbeKind::Viaplay),
    // Americas (10)
    service("hulu", "Hulu", Group::Americas, ProbeKind::Hulu),
    service("max", "Max", Group::Americas, ProbeKind::Max),
    service("peacock", "Peacock", Group::Americas, ProbeKind::Peacock),
    service(
        "paramount-plus",
        "Paramount+",
        Group::Americas,
        ProbeKind::ParamountPlus,
    ),
    service("starz", "Starz", Group::Americas, ProbeKind::Starz),
    service("espn-plus", "ESPN+", Group::Americas, ProbeKind::EspnPlus),
    service("sling-tv", "Sling TV", Group::Americas, ProbeKind::SlingTv),
    service("pluto-tv", "Pluto TV", Group::Americas, ProbeKind::PlutoTv),
    service("crave", "Crave", Group::Americas, ProbeKind::Crave),
    service("cbc-gem", "CBC Gem", Group::Americas, ProbeKind::CbcGem),
    // Oceania (8)
    service("stan", "Stan", Group::Oceania, ProbeKind::Stan),
    service("binge", "Binge", Group::Oceania, ProbeKind::Binge),
    service(
        "kayo-sports",
        "Kayo Sports",
        Group::Oceania,
        ProbeKind::KayoSports,
    ),
    service("nine-now", "9Now", Group::Oceania, ProbeKind::NineNow),
    service("seven-plus", "7plus", Group::Oceania, ProbeKind::SevenPlus),
    service(
        "abc-iview",
        "ABC iview",
        Group::Oceania,
        ProbeKind::AbcIview,
    ),
    service("tvnz-plus", "TVNZ+", Group::Oceania, ProbeKind::TvnzPlus),
    service("threenow", "ThreeNow", Group::Oceania, ProbeKind::ThreeNow),
];

/// Selected services and any unknown ids requested in configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// Selected services in catalog order.
    pub services: Vec<ServiceSpec>,
    /// Requested service ids not found in [`CATALOG`], deduplicated in input order.
    pub unknown: Vec<String>,
}

/// Selects services from [`CATALOG`]: `services` wins over `groups` when non-empty; empty `groups`
/// and empty `services` select the entire catalog.
#[must_use]
pub fn choose(groups: &[Choice], services: &[String]) -> Chosen {
    if !services.is_empty() {
        let mut unknown = Vec::new();
        for wanted in services {
            let trimmed = wanted.trim();
            let exists = CATALOG
                .iter()
                .any(|spec| spec.id.eq_ignore_ascii_case(trimmed));
            let already = unknown
                .iter()
                .any(|seen: &String| seen.eq_ignore_ascii_case(trimmed));
            if !exists && !already {
                unknown.push(trimmed.to_owned());
            }
        }
        let chosen = CATALOG
            .iter()
            .copied()
            .filter(|spec| {
                services
                    .iter()
                    .any(|wanted| spec.id.eq_ignore_ascii_case(wanted.trim()))
            })
            .collect();
        return Chosen {
            services: chosen,
            unknown,
        };
    }

    let chosen = CATALOG
        .iter()
        .copied()
        .filter(|spec| groups.is_empty() || groups.iter().any(|choice| choice.matches(spec.group)))
        .collect();
    Chosen {
        services: chosen,
        unknown: Vec::new(),
    }
}
