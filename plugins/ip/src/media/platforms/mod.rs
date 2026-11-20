//! Individual streaming platform modules (one file per platform), arranged into continent rosters
//! for orchestration.

pub mod abc_iview;
pub mod abema;
pub mod apple_tv;
pub mod bahamut;
pub mod bbc_iplayer;
pub mod bilibili;
pub mod binge;
pub mod canal_plus;
pub mod catchplay;
pub mod cbc_gem;
pub mod channel_4;
pub mod coupang_play;
pub mod crave;
pub mod crunchyroll;
pub mod dazn;
pub mod disney;
pub mod dmm;
pub mod espn_plus;
pub mod france_tv;
pub mod friday_video;
pub mod hamivideo;
pub mod hoy_tv;
pub mod hulu;
pub mod itv_x;
pub mod joyn;
pub mod kayo_sports;
pub mod kktv;
pub mod lemino;
pub mod linetv;
pub mod litv;
pub mod max;
pub mod mewatch;
pub mod molotov;
pub mod movistar_plus;
pub mod mubi;
pub mod mytvsuper;
pub mod myvideo;
pub mod netflix;
pub mod nhk_plus;
pub mod niconico;
pub mod nine_now;
pub mod nlziet;
pub mod paramount_plus;
pub mod peacock;
pub mod pluto_tv;
pub mod prime_video;
pub mod radiko;
pub mod rai_play;
pub mod rakuten_tv;
pub mod rtl_plus;
pub mod seven_plus;
pub mod sky_go;
pub mod sling_tv;
pub mod spotify;
pub mod stan;
pub mod starz;
pub mod threenow;
pub mod tiktok;
pub mod tver;
pub mod tving;
pub mod tvnz_plus;
pub mod twitch;
pub mod u_next;
pub mod viaplay;
pub mod viu;
pub mod wavve;
pub mod youtube;
pub mod zdf;

use std::future::Future;
use std::pin::Pin;

use bench_api::Context;
use bench_api::IpFamily;

use super::result::Group;
use super::result::Output;
use super::result::Verdict;

/// Boxed future returned by a platform's `probe` function.
pub type ProbeFuture<'a> = Pin<Box<dyn Future<Output = Verdict> + Send + 'a>>;

/// Signature of a platform's direct probe entry point.
pub type ProbeFn = for<'a> fn(&'a Context<Output>, IpFamily, Option<&'a str>) -> ProbeFuture<'a>;

/// Static definition of one streaming platform.
#[derive(Clone, Copy)]
pub struct Platform {
    /// Stable lowercase identifier, such as `netflix` or `bbc-iplayer`.
    pub id: &'static str,
    /// Display name shown in the report.
    pub name: &'static str,
    /// Probe entry point over an IP family.
    pub probe: ProbeFn,
}

/// Multinational streaming platforms (12).
pub const GLOBAL: [Platform; 12] = [
    netflix::SPEC,
    youtube::SPEC,
    disney::SPEC,
    prime_video::SPEC,
    apple_tv::SPEC,
    tiktok::SPEC,
    spotify::SPEC,
    dazn::SPEC,
    crunchyroll::SPEC,
    twitch::SPEC,
    rakuten_tv::SPEC,
    mubi::SPEC,
];

/// Asian streaming platforms (24).
pub const ASIA: [Platform; 24] = [
    bilibili::SPEC,
    bahamut::SPEC,
    viu::SPEC,
    mytvsuper::SPEC,
    hoy_tv::SPEC,
    kktv::SPEC,
    litv::SPEC,
    myvideo::SPEC,
    linetv::SPEC,
    friday_video::SPEC,
    catchplay::SPEC,
    hamivideo::SPEC,
    abema::SPEC,
    dmm::SPEC,
    tver::SPEC,
    niconico::SPEC,
    u_next::SPEC,
    lemino::SPEC,
    nhk_plus::SPEC,
    radiko::SPEC,
    wavve::SPEC,
    tving::SPEC,
    coupang_play::SPEC,
    mewatch::SPEC,
];

/// European streaming platforms (14).
pub const EUROPE: [Platform; 14] = [
    bbc_iplayer::SPEC,
    channel_4::SPEC,
    itv_x::SPEC,
    sky_go::SPEC,
    zdf::SPEC,
    joyn::SPEC,
    rtl_plus::SPEC,
    canal_plus::SPEC,
    molotov::SPEC,
    france_tv::SPEC,
    rai_play::SPEC,
    movistar_plus::SPEC,
    nlziet::SPEC,
    viaplay::SPEC,
];

/// North and Latin American streaming platforms (10).
pub const AMERICAS: [Platform; 10] = [
    hulu::SPEC,
    max::SPEC,
    peacock::SPEC,
    paramount_plus::SPEC,
    starz::SPEC,
    espn_plus::SPEC,
    sling_tv::SPEC,
    pluto_tv::SPEC,
    crave::SPEC,
    cbc_gem::SPEC,
];

/// Australian and New Zealand streaming platforms (8).
pub const OCEANIA: [Platform; 8] = [
    stan::SPEC,
    binge::SPEC,
    kayo_sports::SPEC,
    nine_now::SPEC,
    seven_plus::SPEC,
    abc_iview::SPEC,
    tvnz_plus::SPEC,
    threenow::SPEC,
];

/// Returns the continent roster orchestrated under `group`.
#[must_use]
pub const fn in_group(group: Group) -> &'static [Platform] {
    match group {
        Group::Global => &GLOBAL,
        Group::Asia => &ASIA,
        Group::Europe => &EUROPE,
        Group::Americas => &AMERICAS,
        Group::Oceania => &OCEANIA,
    }
}
