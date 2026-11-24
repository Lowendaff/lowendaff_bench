//! Concurrent orchestration of intelligence providers, DNSBL checks, and port-25 mail probes,
//! plus multi-source consensus aggregation for IPv4 and IPv6.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::net::SocketAddr;

use bench_api::Context;
use bench_api::IpFamily;
use bench_api::Step;
use bench_api::Stop;
use bench_host::network::Address;

use super::dnsbl;
use super::mail;
use super::parse;
use super::parse::ProviderData;
use super::parse::ProviderFailure;
use super::providers;
use super::providers::Provider;
use super::result::DnsblSummary;
use super::result::FactorSummary;
use super::result::Failure;
use super::result::FamilyReport;
use super::result::GeoVote;
use super::result::MailCheck;
use super::result::Origin;
use super::result::Output;
use super::result::RiskFactor;
use super::result::Settings;

/// Validated runtime endpoints passed into [`measure`].
#[derive(Debug, Clone, Default)]
pub struct Endpoints {
    /// Optional base URL override for intelligence providers.
    pub base_url: Option<String>,
    /// Optional loopback DNS server override for DNSBL checks.
    pub dns_server: Option<SocketAddr>,
    /// Optional loopback SMTP server override for port-25 checks.
    pub smtp_server: Option<SocketAddr>,
}

/// Runs `ip.quality` across the host's reachable public IPv4 and IPv6 addresses.
pub async fn measure(
    ctx: &Context<Output>,
    settings: Settings,
    endpoints: &Endpoints,
) -> Result<Output, Stop<Failure>> {
    let network = &ctx.host().network;
    let ipv4_reachable = network.ipv4.reachable();
    let ipv6_reachable = network.ipv6.reachable();
    if !ipv4_reachable && !ipv6_reachable {
        return Err(Stop::Skipped(Failure::NoFamily));
    }

    let v4_ip = if ipv4_reachable {
        found_ip(&network.ipv4.address)
    } else {
        None
    };
    let v6_ip = if ipv6_reachable {
        found_ip(&network.ipv6.address)
    } else {
        None
    };
    if v4_ip.is_none() && v6_ip.is_none() {
        return Err(Stop::Skipped(Failure::NoPublicAddress));
    }
    if ctx.cancel_token().is_cancelled() {
        return Err(Stop::Failed(Failure::Cancelled));
    }

    let mut output = Output {
        settings: settings.clone(),
        ipv4_reachable,
        ipv6_reachable,
        ipv4: None,
        ipv6: None,
    };

    let targets = [
        (IpFamily::Ipv4, v4_ip, ipv6_reachable, false),
        (IpFamily::Ipv6, v6_ip, ipv4_reachable, true),
    ];
    for (family, maybe_ip, other_reachable, ipv6) in targets {
        let Some(ip) = maybe_ip else {
            continue;
        };
        if ctx.cancel_token().is_cancelled() {
            return Err(Stop::Failed(Failure::Cancelled));
        }
        let fallback =
            other_reachable.then_some(if ipv6 { IpFamily::Ipv4 } else { IpFamily::Ipv6 });

        ctx.progress(Step::IpQuality {
            stage: "intelligence",
            ipv6,
        });
        let base_url = endpoints.base_url.as_deref();
        let intel_tasks = providers::ALL.into_iter().map(|spec| async move {
            let res = (spec.probe)(ctx, ip, family, fallback, base_url).await;
            (spec, res)
        });
        let cancel = ctx.cancel_token();
        let intel_outcomes =
            bench_api::run_bounded(providers::ALL.len(), &cancel, intel_tasks).await;
        if cancel.is_cancelled() {
            return Err(Stop::Failed(Failure::Cancelled));
        }

        let dnsbl_summary = if settings.dnsbl {
            ctx.progress(Step::IpQuality {
                stage: "dnsbl",
                ipv6,
            });
            let zones = dnsbl::zones_for(ip);
            let dns_server = endpoints.dns_server;
            let zone_tasks = zones
                .into_iter()
                .map(|zone| dnsbl::check_zone(ctx, ip, zone, dns_server));
            let outcomes =
                bench_api::run_bounded(dnsbl::DNSBL_CONCURRENCY, &cancel, zone_tasks).await;
            if cancel.is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            Some(dnsbl::summarize(outcomes))
        } else {
            None
        };

        let mail_checks = if settings.mail {
            ctx.progress(Step::IpQuality {
                stage: "mail",
                ipv6,
            });
            let smtp = endpoints.smtp_server;
            let mail_tasks = mail::PROVIDERS
                .into_iter()
                .map(|target| mail::check_provider(ctx, family, target, smtp));
            let checks = bench_api::run_bounded(mail::PROVIDERS.len(), &cancel, mail_tasks).await;
            if cancel.is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            checks
        } else {
            Vec::new()
        };

        let report = aggregate(ip, &intel_outcomes, dnsbl_summary, mail_checks);
        if ipv6 {
            output.ipv6 = Some(Box::new(report));
        } else {
            output.ipv4 = Some(Box::new(report));
        }
        ctx.publish(&output);
    }

    Ok(output)
}

fn found_ip(address: &Address) -> Option<IpAddr> {
    match address {
        Address::Found { address, .. } => Some(*address),
        _ => None,
    }
}

/// Combines provider responses into one [`FamilyReport`].
#[must_use]
pub fn aggregate(
    address: IpAddr,
    intel: &[(Provider, Result<ProviderData, ProviderFailure>)],
    dnsbl: Option<DnsblSummary>,
    mail: Vec<MailCheck>,
) -> FamilyReport {
    let find_ok = |id: &str| -> Option<&ProviderData> {
        intel
            .iter()
            .find_map(|(spec, res)| (spec.id == id).then(|| res.as_ref().ok()).flatten())
    };

    let priority_ids = [
        "ipapi",
        "ripestat",
        "ipinfo",
        "ipwhois",
        "proxycheck",
        "shodan",
    ];
    let asn = priority_ids
        .iter()
        .find_map(|id| find_ok(id).and_then(|d| d.asn));
    let org = match asn {
        Some(chosen) => priority_ids
            .iter()
            .find_map(|id| {
                find_ok(id)
                    .filter(|d| d.asn == Some(chosen))
                    .and_then(|d| d.org.clone())
            })
            .or_else(|| {
                priority_ids.iter().find_map(|id| {
                    find_ok(id)
                        .filter(|d| d.asn.is_none())
                        .and_then(|d| d.org.clone())
                })
            }),
        None => priority_ids
            .iter()
            .find_map(|id| find_ok(id).and_then(|d| d.org.clone())),
    };
    let hostname = ["ipinfo", "shodan"]
        .iter()
        .find_map(|id| find_ok(id).and_then(|d| d.hostname.clone()));
    let rir = ["ripestat", "ipapi"]
        .iter()
        .find_map(|id| find_ok(id).and_then(|d| d.rir.clone()));
    let registered_country = ["ripestat", "ipapi"]
        .iter()
        .find_map(|id| find_ok(id).and_then(|d| d.registered_country.clone()));
    let usage = ["ipapi", "proxycheck", "shodan"]
        .iter()
        .find_map(|id| find_ok(id).and_then(|d| d.usage));

    let (geo_country, geo_votes, geo_total) = tally_geo(intel);
    let origin = match (&registered_country, &geo_country) {
        (Some(reg), Some(geo)) if reg == geo => Origin::Native {
            country: geo.clone(),
        },
        (Some(reg), Some(geo)) => Origin::Broadcast {
            registered: reg.clone(),
            located: geo.clone(),
        },
        _ => Origin::Unknown,
    };

    let mut scores = Vec::new();
    let mut notes = Vec::new();
    for (spec, res) in intel {
        match res {
            Ok(data) => {
                if spec.has_score
                    && let Some(score) = &data.score
                {
                    scores.push((spec.name, score.clone()));
                }
            }
            Err(fail) => {
                let scrubbed = match fail {
                    ProviderFailure::Unexpected(why) => {
                        ProviderFailure::Unexpected(parse::scrub_ip(why, address))
                    }
                    other => other.clone(),
                };
                if let ProviderFailure::Unexpected(why) = &scrubbed {
                    notes.push((spec.name, why.clone()));
                }
                if spec.has_score {
                    scores.push((spec.name, scrubbed.into_score()));
                }
            }
        }
    }

    let factors = RiskFactor::ALL
        .into_iter()
        .map(|factor| {
            let mut checked = 0;
            let mut flagged_by = Vec::new();
            for (spec, res) in intel {
                if let Ok(data) = res
                    && let Some((_, hit)) = data.flags.iter().find(|(f, _)| *f == factor)
                {
                    checked += 1;
                    if *hit {
                        flagged_by.push(spec.name);
                    }
                }
            }
            FactorSummary {
                factor,
                checked,
                flagged_by,
            }
        })
        .collect();

    FamilyReport {
        address,
        asn,
        org,
        hostname,
        rir,
        registered_country,
        geo_country,
        geo_votes,
        geo_total,
        origin,
        usage,
        scores,
        factors,
        dnsbl,
        mail,
        notes,
    }
}

fn tally_geo(
    intel: &[(Provider, Result<ProviderData, ProviderFailure>)],
) -> (Option<String>, Vec<GeoVote>, usize) {
    let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut total = 0;
    for (order, (_, res)) in intel.iter().enumerate() {
        if let Ok(data) = res
            && let Some(code) = &data.geo_country
        {
            total += 1;
            counts
                .entry(code.clone())
                .and_modify(|(c, _)| *c += 1)
                .or_insert((1, order));
        }
    }
    let mut ordered: Vec<(String, usize, usize)> = counts
        .into_iter()
        .map(|(code, (count, first))| (code, count, first))
        .collect();
    ordered.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
    let winner = ordered.first().map(|(code, _, _)| code.clone());
    let votes = ordered
        .into_iter()
        .map(|(country, count, _)| GeoVote { country, count })
        .collect();
    (winner, votes, total)
}
