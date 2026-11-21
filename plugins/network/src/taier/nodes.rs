//! Parsing, validation, and location/operator selection for the Taier server list.
//!
//! Matches the exact validation rules of `scripts/taier-servers.sh`, verified against shared
//! fixtures in `plugins/network/tests/fixtures/taier-lists/`.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use serde::Deserialize;
use serde::Serialize;

use super::result::Operator;
use super::result::OperatorChoice;

/// Supported list schema version.
pub const VERSION: u64 = 1;

/// Maximum size in bytes of the Taier server list (`64 KiB`).
pub const LIMIT: usize = 64 * 1024;

/// Maximum locations allowed in one list.
pub const MOST_LOCATIONS: usize = 400;

/// Maximum candidate nodes allowed inside one location.
pub const MOST_NODES: usize = 32;

/// Canonical ordering of default locations (`Beijing`, `Shanghai`, `Guangzhou`).
const DEFAULT_ORDER: [&str; 3] = ["beijing", "shanghai", "guangdong-guangzhou"];

/// Why a Taier server list cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ListError {
    /// File exceeds [`LIMIT`] bytes.
    TooLarge,
    /// Invalid JSON or wrong top-level field types.
    Format(String),
    /// Schema version is newer than [`VERSION`].
    Newer(u64),
    /// Schema version is `0` or otherwise invalid.
    Version(u64),
    /// `locations` is empty.
    Empty,
    /// `locations` holds more than [`MOST_LOCATIONS`] entries.
    TooMany(usize),
    /// Entry at 1-based position `usize` violated [`Rule`].
    Location(usize, Rule),
    /// Two locations share the same `id`.
    Duplicate(String),
}

/// Specific validation rule broken by a location entry in the Taier server list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Rule {
    /// `id` must match `[a-z0-9-]{1,40}` and not be `"all"`.
    Id,
    /// `province` must be 1..=40 printable ASCII characters.
    Province,
    /// `city` must be 1..=40 printable ASCII characters.
    City,
    /// `nodes` must hold 1..=32 entries.
    Nodes,
    /// Node `operator` must be a known carrier code.
    Operator,
    /// Node `ip` must be a canonical IPv4 address.
    Ip,
    /// Node `port` must be in `1024..=65535`.
    Port,
    /// Two nodes inside the same location share the same `ip:port`.
    DuplicateEndpoint,
}

/// One validated candidate node inside a [`Location`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Carrier network.
    pub operator: Operator,
    /// Node IPv4 address.
    pub ip: Ipv4Addr,
    /// Node TCP port (`1024..=65535`).
    pub port: u16,
}

/// One validated city/region entry from the Taier server list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// Stable lowercase slug (e.g. `beijing` or `jiangsu-nanjing`).
    pub id: String,
    /// Printable ASCII province/region name.
    pub province: String,
    /// Printable ASCII city name.
    pub city: String,
    /// Whether selected when no `--taier-cities` filter is given.
    pub default: bool,
    /// Candidate carrier nodes in fallback order.
    pub nodes: Vec<Node>,
}

impl Location {
    /// Human-readable place label (`Beijing` when `city == province`, else `Nanjing, Jiangsu`).
    #[must_use]
    pub fn display_place(&self) -> String {
        if self.city.eq_ignore_ascii_case(&self.province) {
            self.city.clone()
        } else {
            format!("{}, {}", self.city, self.province)
        }
    }
}

/// One `(location, operator)` slot selected to run, with all candidate nodes in fallback order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSlot {
    /// Location identifier.
    pub id: String,
    /// Printable ASCII province name.
    pub province: String,
    /// Printable ASCII city name.
    pub city: String,
    /// Carrier network.
    pub operator: Operator,
    /// Non-empty candidate endpoints `(ip, port)` in fallback order.
    pub candidates: Vec<(Ipv4Addr, u16)>,
}

/// Selected `(location, operator)` slots and any unmatched city/province tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// Resolved carrier filter.
    pub operators: Vec<Operator>,
    /// Selected `(location, operator)` slots in execution order.
    pub slots: Vec<TargetSlot>,
    /// Requested city/province tokens that matched no location.
    pub unknown: Vec<String>,
}

/// Parses and validates Taier server list bytes.
pub fn parse(bytes: &[u8]) -> Result<Vec<Location>, ListError> {
    #[derive(Deserialize)]
    struct VersionOnly {
        version: u64,
    }
    #[derive(Deserialize)]
    struct RawList {
        locations: Vec<RawLocation>,
    }
    #[derive(Deserialize)]
    struct RawLocation {
        id: String,
        province: String,
        city: String,
        #[serde(default)]
        default: bool,
        nodes: Vec<RawNode>,
    }
    #[derive(Deserialize)]
    struct RawNode {
        operator: String,
        ip: String,
        port: u64,
    }

    if bytes.len() > LIMIT {
        return Err(ListError::TooLarge);
    }
    let ver: VersionOnly =
        serde_json::from_slice(bytes).map_err(|err| ListError::Format(err.to_string()))?;
    match ver.version {
        VERSION => {}
        newer if newer > VERSION => return Err(ListError::Newer(newer)),
        other => return Err(ListError::Version(other)),
    }
    let raw: RawList =
        serde_json::from_slice(bytes).map_err(|err| ListError::Format(err.to_string()))?;
    if raw.locations.is_empty() {
        return Err(ListError::Empty);
    }
    if raw.locations.len() > MOST_LOCATIONS {
        return Err(ListError::TooMany(raw.locations.len()));
    }

    let mut seen_ids = BTreeSet::new();
    let mut out = Vec::with_capacity(raw.locations.len());
    for (idx, loc) in raw.locations.into_iter().enumerate() {
        let pos = idx + 1;
        if !valid_id(&loc.id) {
            return Err(ListError::Location(pos, Rule::Id));
        }
        if !ascii_printable(&loc.province, 40) {
            return Err(ListError::Location(pos, Rule::Province));
        }
        if !ascii_printable(&loc.city, 40) {
            return Err(ListError::Location(pos, Rule::City));
        }
        if loc.nodes.is_empty() || loc.nodes.len() > MOST_NODES {
            return Err(ListError::Location(pos, Rule::Nodes));
        }
        let mut endpoints = BTreeSet::new();
        let mut nodes = Vec::with_capacity(loc.nodes.len());
        for node in loc.nodes {
            let Some(operator) = parse_operator(&node.operator) else {
                return Err(ListError::Location(pos, Rule::Operator));
            };
            let Some(ip) = parse_canonical_ipv4(&node.ip) else {
                return Err(ListError::Location(pos, Rule::Ip));
            };
            if !(1024..=65535).contains(&node.port) {
                return Err(ListError::Location(pos, Rule::Port));
            }
            let port = u16::try_from(node.port).unwrap_or(65499);
            if !endpoints.insert((ip, port)) {
                return Err(ListError::Location(pos, Rule::DuplicateEndpoint));
            }
            nodes.push(Node { operator, ip, port });
        }
        if !seen_ids.insert(loc.id.clone()) {
            return Err(ListError::Duplicate(loc.id));
        }
        out.push(Location {
            id: loc.id,
            province: loc.province,
            city: loc.city,
            default: loc.default,
            nodes,
        });
    }
    Ok(out)
}

/// Resolves requested `cities` (IDs, city names, province names, or `all`) and `operators` into
/// ordered `(location, operator)` [`TargetSlot`]s.
#[must_use]
pub fn choose(locations: &[Location], cities: &[String], operators: &[OperatorChoice]) -> Chosen {
    let resolved_ops = resolve_operators(operators);
    let mut unknown: Vec<String> = Vec::new();
    let chosen_locs: Vec<&Location> = if cities.is_empty() {
        let mut defaults: Vec<&Location> = locations.iter().filter(|loc| loc.default).collect();
        defaults.sort_by_key(|loc| {
            DEFAULT_ORDER
                .iter()
                .position(|&id| id == loc.id)
                .unwrap_or(DEFAULT_ORDER.len())
        });
        defaults
    } else {
        let mut want_all = false;
        for token in cities {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.eq_ignore_ascii_case("all") {
                want_all = true;
            } else if !locations.iter().any(|loc| matches_token(loc, trimmed))
                && !unknown.iter().any(|u| u.eq_ignore_ascii_case(trimmed))
            {
                unknown.push(trimmed.to_owned());
            }
        }
        if want_all {
            locations.iter().collect()
        } else {
            locations
                .iter()
                .filter(|loc| {
                    cities
                        .iter()
                        .map(|s| s.trim())
                        .any(|tok| !tok.is_empty() && matches_token(loc, tok))
                })
                .collect()
        }
    };

    let mut slots = Vec::new();
    for loc in chosen_locs {
        for &op in &resolved_ops {
            let candidates: Vec<(Ipv4Addr, u16)> = loc
                .nodes
                .iter()
                .filter(|n| n.operator == op)
                .map(|n| (n.ip, n.port))
                .collect();
            if !candidates.is_empty() {
                slots.push(TargetSlot {
                    id: loc.id.clone(),
                    province: loc.province.clone(),
                    city: loc.city.clone(),
                    operator: op,
                    candidates,
                });
            }
        }
    }

    Chosen {
        operators: resolved_ops,
        slots,
        unknown,
    }
}

fn resolve_operators(choices: &[OperatorChoice]) -> Vec<Operator> {
    if choices.is_empty() {
        return Operator::DEFAULTS.to_vec();
    }
    if choices.contains(&OperatorChoice::All) {
        return Operator::ALL.to_vec();
    }
    let mapped: Vec<Operator> = choices
        .iter()
        .map(|choice| match choice {
            OperatorChoice::Telecom => Operator::Telecom,
            OperatorChoice::Unicom => Operator::Unicom,
            OperatorChoice::Mobile => Operator::Mobile,
            OperatorChoice::Cernet => Operator::Cernet,
            OperatorChoice::Cbn => Operator::Cbn,
            OperatorChoice::Other => Operator::Other,
            OperatorChoice::All => unreachable!(),
        })
        .collect();
    Operator::ALL
        .into_iter()
        .filter(|op| mapped.contains(op))
        .collect()
}

fn matches_token(loc: &Location, token: &str) -> bool {
    loc.id.eq_ignore_ascii_case(token)
        || loc.city.eq_ignore_ascii_case(token)
        || loc.province.eq_ignore_ascii_case(token)
}

fn valid_id(id: &str) -> bool {
    (1..=40).contains(&id.len())
        && id != "all"
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn ascii_printable(text: &str, max_len: usize) -> bool {
    (1..=max_len).contains(&text.len()) && text.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

fn parse_operator(raw: &str) -> Option<Operator> {
    match raw {
        "telecom" => Some(Operator::Telecom),
        "unicom" => Some(Operator::Unicom),
        "mobile" => Some(Operator::Mobile),
        "cernet" => Some(Operator::Cernet),
        "cbn" => Some(Operator::Cbn),
        "other" => Some(Operator::Other),
        _ => None,
    }
}

/// Parses a canonical dotted-quad IPv4 address without leading zeros (matching `list-rules.jq`).
fn parse_canonical_ipv4(raw: &str) -> Option<Ipv4Addr> {
    let ip: Ipv4Addr = raw.parse().ok()?;
    (ip.to_string() == raw).then_some(ip)
}
