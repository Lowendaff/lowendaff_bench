//! Individual IP intelligence and risk-scoring provider modules (one file per provider),
//! orchestrated concurrently for each public IP family.

pub mod dbip;
pub mod greynoise;
pub mod ipapi;
pub mod ipinfo;
pub mod ipwhois;
pub mod proxycheck;
pub mod ripestat;
pub mod scamalytics;
pub mod shodan;

use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;

use bench_api::Context;
use bench_api::IpFamily;

use super::parse::ProviderData;
use super::parse::ProviderFailure;
use super::result::Output;

/// Boxed future returned by a provider's `probe` function.
pub type ProbeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ProviderData, ProviderFailure>> + Send + 'a>>;

/// Signature of a provider's probe entry point.
pub type ProbeFn = for<'a> fn(
    &'a Context<Output>,
    IpAddr,
    IpFamily,
    Option<IpFamily>,
    Option<&'a str>,
) -> ProbeFuture<'a>;

/// Static definition of one IP intelligence / reputation provider.
#[derive(Clone, Copy)]
pub struct Provider {
    /// Stable lowercase identifier, such as `ipapi` or `scamalytics`.
    pub id: &'static str,
    /// Display name shown in the report.
    pub name: &'static str,
    /// Whether this provider appears in the `Risk scores` table.
    pub has_score: bool,
    /// Probe entry point.
    pub probe: ProbeFn,
}

/// All nine unauthenticated direct intelligence providers in report order.
pub const ALL: [Provider; 9] = [
    scamalytics::SPEC,
    proxycheck::SPEC,
    ipapi::SPEC,
    greynoise::SPEC,
    shodan::SPEC,
    ripestat::SPEC,
    ipinfo::SPEC,
    dbip::SPEC,
    ipwhois::SPEC,
];
