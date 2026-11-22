//! The target list, the one in this repository or a file in its format: the fixed targets that the
//! item traces, and the rules that the item checks before it uses a list, as CI checks them in
//! every pull request that changes the repository's.

use std::net::IpAddr;

use serde::Deserialize;
use serde::Serialize;

use super::lists;
use super::lists::ListError;
use super::lists::Rule;

/// The most targets that a list may hold.
pub const MOST_TARGETS: usize = 200;

/// The longest id, in characters.
pub const LONGEST_ID: usize = 40;

/// The longest name, in characters.
pub const LONGEST_NAME: usize = 40;

/// The ids of the cache servers, which the item looks up at each run, so that no target of a list
/// may take them.
pub const SERVICES: [&str; 2] = ["youtube", "netflix"];

/// A group of targets, in the report's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Group {
    /// The nearest YouTube and Netflix cache servers, which no list holds.
    Services,
    /// Targets in mainland China.
    China,
    /// Targets elsewhere.
    International,
}

/// A target of the list, checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Target {
    /// Its id, unique in the list, such as `pek-4809`.
    pub id: String,
    /// Its group: China or international.
    pub group: Group,
    /// Its name, such as `Beijing Telecom CN2`.
    pub name: String,
    /// The AS number of its network.
    pub asn: u32,
    /// The DNS name or IPv4 address that the item traces over IPv4.
    pub ipv4: String,
    /// The DNS name or IPv6 address that the item traces over IPv6, if any.
    pub ipv6: Option<String>,
    /// Whether the item traces it unless told otherwise.
    pub default: bool,
}

/// The list as it is written, after its version.
#[derive(Deserialize)]
struct Written {
    targets: Vec<WrittenTarget>,
}

/// A target as it is written.
#[derive(Deserialize)]
struct WrittenTarget {
    id: String,
    group: String,
    name: String,
    asn: u64,
    ipv4: String,
    #[serde(default)]
    ipv6: Option<String>,
    #[serde(default)]
    default: bool,
}

/// Reads a list, and checks every schema rule.
///
/// # Errors
///
/// When the list is too large, not of the format, of another version, or breaks a rule.
pub fn parse(bytes: &[u8]) -> Result<Vec<Target>, ListError> {
    parse_inner(bytes, true)
}

/// Reads a list and enforces endpoint routing safety when `allow_local` is `false` (remote URLs),
/// rejecting loopback, `localhost`, private, link-local, metadata, and non-global IPv6 addresses.
///
/// # Errors
///
/// When the list is too large, not of the format, of another version, or breaks a rule.
pub fn parse_for_source(bytes: &[u8], allow_local: bool) -> Result<Vec<Target>, ListError> {
    parse_inner(bytes, allow_local)
}

fn parse_inner(bytes: &[u8], allow_local: bool) -> Result<Vec<Target>, ListError> {
    lists::versioned(bytes)?;
    let written: Written =
        serde_json::from_slice(bytes).map_err(|err| ListError::Format(err.to_string()))?;
    if written.targets.len() > MOST_TARGETS {
        return Err(ListError::TooMany(written.targets.len()));
    }
    let mut targets: Vec<Target> = Vec::with_capacity(written.targets.len());
    for (index, target) in written.targets.into_iter().enumerate() {
        let target =
            check(target, allow_local).map_err(|rule| ListError::Entry(index + 1, rule))?;
        if targets.iter().any(|known| known.id == target.id) {
            return Err(ListError::Duplicate(target.id));
        }
        targets.push(target);
    }
    Ok(targets)
}

/// A written target, checked against every rule.
fn check(target: WrittenTarget, allow_local: bool) -> Result<Target, Rule> {
    let id_ok = (1..=LONGEST_ID).contains(&target.id.len())
        && target
            .id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !SERVICES.contains(&target.id.as_str());
    if !id_ok {
        return Err(Rule::Id);
    }
    let group = match target.group.as_str() {
        "china" => Group::China,
        "international" => Group::International,
        _ => return Err(Rule::Group),
    };
    if !lists::printable(&target.name, LONGEST_NAME) {
        return Err(Rule::Name);
    }
    let asn = u32::try_from(target.asn)
        .ok()
        .filter(|asn| *asn > 0)
        .ok_or(Rule::Asn)?;
    if !name_of(&target.ipv4, false, allow_local) {
        return Err(Rule::Ipv4);
    }
    if let Some(ipv6) = &target.ipv6
        && !name_of(ipv6, true, allow_local)
    {
        return Err(Rule::Ipv6);
    }
    Ok(Target {
        id: target.id,
        group,
        name: target.name,
        asn,
        ipv4: target.ipv4,
        ipv6: target.ipv6,
        default: target.default,
    })
}

/// A DNS name, or an address of the family that the name is traced over: IPv6 when `ipv6` is set.
fn name_of(name: &str, ipv6: bool, allow_local: bool) -> bool {
    match name.parse::<IpAddr>() {
        Ok(address) => {
            address.is_ipv6() == ipv6 && (allow_local || crate::endpoint::is_routable_ip(address))
        }
        Err(_) => lists::host(name) && (allow_local || lists::is_remote_safe_host(name)),
    }
}

/// What a configuration may choose by group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    /// The nearest YouTube and Netflix cache servers.
    Services,
    /// Every target of the list in China.
    China,
    /// Every target of the list elsewhere.
    International,
    /// Both cache servers and every target of the list.
    All,
}

/// What to trace, and what was asked for that the list does not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// Whether to trace YouTube's nearest cache server.
    pub youtube: bool,
    /// Whether to trace Netflix's nearest cache server.
    pub netflix: bool,
    /// The targets of the list, in its order.
    pub targets: Vec<Target>,
    /// The ids asked for that neither the list nor the cache servers have, as they were given.
    pub unknown: Vec<String>,
}

/// Whether `ids` or `groups` choose the cache servers, YouTube's and Netflix's: by their ids, by
/// their group, by `all`, or by default.
#[must_use]
pub fn services(groups: &[Choice], ids: &[String]) -> (bool, bool) {
    if !ids.is_empty() {
        let asked = |service: &str| ids.iter().any(|id| id == service);
        return (asked("youtube"), asked("netflix"));
    }
    let both = groups.is_empty()
        || groups
            .iter()
            .any(|group| matches!(group, Choice::Services | Choice::All));
    (both, both)
}

/// What to trace: the targets with the `ids` given, the cache servers among them, else the
/// targets of the `groups` given, else both cache servers and the list's defaults.
#[must_use]
pub fn choose(targets: &[Target], groups: &[Choice], ids: &[String]) -> Chosen {
    let (youtube, netflix) = services(groups, ids);
    if !ids.is_empty() {
        let unknown = ids
            .iter()
            .filter(|wanted| {
                !SERVICES.contains(&wanted.as_str())
                    && !targets.iter().any(|target| target.id == **wanted)
            })
            .cloned()
            .collect();
        let chosen = targets
            .iter()
            .filter(|target| ids.contains(&target.id))
            .cloned()
            .collect();
        return Chosen {
            youtube,
            netflix,
            targets: chosen,
            unknown,
        };
    }
    let chosen = targets
        .iter()
        .filter(|target| {
            if groups.is_empty() {
                return target.default;
            }
            groups.iter().any(|group| match group {
                Choice::All => true,
                Choice::China => target.group == Group::China,
                Choice::International => target.group == Group::International,
                Choice::Services => false,
            })
        })
        .cloned()
        .collect();
    Chosen {
        youtube,
        netflix,
        targets: chosen,
        unknown: Vec::new(),
    }
}
