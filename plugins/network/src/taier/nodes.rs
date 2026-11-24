//! Parsing, validation, and location/operator selection for the Taier server list.
//!
//! Matches the exact validation rules of `Lowendaff/bench_data` (`scripts/taier-servers.sh`),
//! verified against shared fixtures in `plugins/network/tests/fixtures/taier-lists/`.

use std::collections::BTreeSet;
use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de;

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

/// Parses and validates a remote or embedded Taier server list, rejecting loopback, private,
/// link-local, metadata, multicast, and reserved IPv4 addresses.
pub fn parse(bytes: &[u8]) -> Result<Vec<Location>, ListError> {
    parse_for_source(bytes, false)
}

/// Parses and validates Taier server list bytes; `allow_loopback` permits `127.0.0.0/8` only when
/// reading an explicit local file path override (such as smoke tests).
pub fn parse_for_source(bytes: &[u8], allow_loopback: bool) -> Result<Vec<Location>, ListError> {
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
            let allowed_ip = (allow_loopback && ip.is_loopback())
                || bench_api::is_routable_endpoint_ip(std::net::IpAddr::V4(ip));
            if !allowed_ip {
                return Err(ListError::Location(pos, Rule::Ip));
            }
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

/// Why a `<place>[:<op>[+<op>...]]` target selector failed syntax validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorError {
    /// Selector string was empty or whitespace-only.
    Empty,
    /// Malformed `<place>` or multiple `:` / `/` separators.
    Format(String),
    /// Missing `<place>` before `:` (e.g. `:ct`).
    EmptyPlace(String),
    /// Empty carrier list or trailing `+` after `:` (e.g. `beijing:` or `beijing:ct+`).
    EmptyOperators(String),
    /// Unknown carrier token after `:`.
    UnknownOperator {
        /// Original selector token.
        selector: String,
        /// Unrecognized carrier part.
        operator: String,
    },
}

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty Taier target selector"),
            Self::Format(raw) => write!(
                f,
                "invalid Taier selector `{raw}`: expected <place>[:<op>[+<op>...]] \
                 (such as beijing:cu or guangdong/shenzhen:ct+cm)"
            ),
            Self::EmptyPlace(raw) => {
                write!(f, "invalid Taier selector `{raw}`: missing location name")
            }
            Self::EmptyOperators(raw) => write!(
                f,
                "invalid Taier selector `{raw}`: expected carrier code after `:` \
                 (ct, cu, cm, edu, cbn, other, or all)"
            ),
            Self::UnknownOperator { selector, operator } => write!(
                f,
                "unknown carrier `{operator}` in Taier selector `{selector}`: expected \
                 telecom/ct, unicom/cu, mobile/cm, cernet/edu, cbn, other, or all"
            ),
        }
    }
}

impl std::error::Error for SelectorError {}

/// One validated `<place>[:<op>[+<op>...]]` target selector for `network.taier`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    /// Location token: `all`, `<province>/<city>`, `<province>`, `<id>`, or `<city>`.
    pub place: String,
    /// Explicit per-location carrier filter; empty inherits `Taier::operators` or `DEFAULTS`.
    pub operators: Vec<OperatorChoice>,
}

impl Selector {
    /// Validates and parses one `<place>[:<op>[+<op>...]]` selector token.
    pub fn try_new(raw: &str) -> Result<Self, SelectorError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(SelectorError::Empty);
        }
        let (place_part, ops_part) = match trimmed.split_once(':') {
            Some((left, right)) => {
                if right.contains(':') {
                    return Err(SelectorError::Format(trimmed.to_owned()));
                }
                (left.trim(), Some(right.trim()))
            }
            None => (trimmed, None),
        };
        if place_part.is_empty() {
            return Err(SelectorError::EmptyPlace(trimmed.to_owned()));
        }
        if let Some((prov, city)) = place_part.split_once('/')
            && (prov.trim().is_empty() || city.trim().is_empty() || city.contains('/'))
        {
            return Err(SelectorError::Format(trimmed.to_owned()));
        }
        let mut operators = Vec::new();
        if let Some(ops_str) = ops_part {
            if ops_str.is_empty() {
                return Err(SelectorError::EmptyOperators(trimmed.to_owned()));
            }
            for part in ops_str.split('+') {
                let op_tok = part.trim();
                if op_tok.is_empty() {
                    return Err(SelectorError::EmptyOperators(trimmed.to_owned()));
                }
                let Some(choice) = OperatorChoice::try_new(op_tok) else {
                    return Err(SelectorError::UnknownOperator {
                        selector: trimmed.to_owned(),
                        operator: op_tok.to_owned(),
                    });
                };
                if !operators.contains(&choice) {
                    operators.push(choice);
                }
            }
        }
        let normalized_place = match place_part.split_once('/') {
            Some((prov, city)) => format!("{}/{}", prov.trim(), city.trim()),
            None => place_part.to_owned(),
        };
        Ok(Self {
            place: normalized_place,
            operators,
        })
    }
}

impl FromStr for Selector {
    type Err = SelectorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.place)?;
        if !self.operators.is_empty() {
            f.write_str(":")?;
            if self.operators.contains(&OperatorChoice::All) {
                f.write_str("all")?;
            } else {
                for (idx, choice) in self.operators.iter().enumerate() {
                    if idx > 0 {
                        f.write_str("+")?;
                    }
                    f.write_str(choice.short_code())?;
                }
            }
        }
        Ok(())
    }
}

impl Serialize for Selector {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Selector {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(&raw).map_err(de::Error::custom)
    }
}

/// Lowercase ASCII slug turning spaces into `-` (e.g. `"Inner Mongolia"` -> `"inner-mongolia"`).
#[must_use]
pub fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if (ch == ' ' || ch == '-' || ch == '_') && !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    out
}

/// Resolves requested `cities` (IDs, city names, province names, `<province>/<city>`, or `all`)
/// and `operators` into ordered `(location, operator)` [`TargetSlot`]s.
#[must_use]
pub fn choose(locations: &[Location], cities: &[String], operators: &[OperatorChoice]) -> Chosen {
    select(locations, &[], cities, operators)
}

/// Resolves unified `selections` (`<place>[:<op>[+<op>...]]`), `cities`, and fallback `operators`
/// into deduplicated `(location, operator)` [`TargetSlot`]s in execution order.
#[must_use]
pub fn select(
    locations: &[Location],
    selections: &[Selector],
    cities: &[String],
    operators: &[OperatorChoice],
) -> Chosen {
    let fallback_ops = resolve_operators(operators);
    let city_selectors: Vec<Selector> = cities
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|place| Selector {
            place: place.to_owned(),
            operators: Vec::new(),
        })
        .collect();

    if selections.is_empty() && city_selectors.is_empty() {
        let mut defaults: Vec<&Location> = locations.iter().filter(|loc| loc.default).collect();
        defaults.sort_by_key(|loc| {
            DEFAULT_ORDER
                .iter()
                .position(|&id| id == loc.id)
                .unwrap_or(DEFAULT_ORDER.len())
        });
        let mut slots = Vec::new();
        for loc in defaults {
            append_location_slots(&mut slots, &mut BTreeSet::new(), loc, &fallback_ops);
        }
        return Chosen {
            operators: fallback_ops,
            slots,
            unknown: Vec::new(),
        };
    }

    let mut unknown: Vec<String> = Vec::new();
    let mut seen_slots: BTreeSet<(String, Operator)> = BTreeSet::new();
    let mut seen_ops: BTreeSet<Operator> = BTreeSet::new();
    let mut slots = Vec::new();
    let all_selectors = selections.iter().chain(city_selectors.iter());

    for sel in all_selectors {
        let matched = match_locations(locations, &sel.place);
        if matched.is_empty() {
            if !unknown.iter().any(|u| u.eq_ignore_ascii_case(&sel.place)) {
                unknown.push(sel.place.clone());
            }
            continue;
        }
        let ops = if sel.operators.is_empty() {
            fallback_ops.clone()
        } else {
            resolve_operators(&sel.operators)
        };
        for &op in &ops {
            seen_ops.insert(op);
        }
        for loc in matched {
            append_location_slots(&mut slots, &mut seen_slots, loc, &ops);
        }
    }

    let summary_ops = if seen_ops.is_empty() {
        fallback_ops
    } else {
        Operator::ALL
            .into_iter()
            .filter(|op| seen_ops.contains(op))
            .collect()
    };

    Chosen {
        operators: summary_ops,
        slots,
        unknown,
    }
}

/// Finds every [`Location`] matching `place`:
/// 1. `"all"` -> all locations.
/// 2. `<province>/<city>` -> exact province + city match (`city == "all"` or `"*"` matches the
///    whole province).
/// 3. Bare `<province>` -> every location in that province (prioritized ahead of homonymous
///    autonomous prefectures such as `Qinghai/Hainan`).
/// 4. Bare `<id>` -> exact location `id` match (such as `guangdong-shenzhen` or `qinghai-hainan`).
/// 5. Bare `<city>` -> every location with that city name (such as `shenzhen` or `chengdu`).
#[must_use]
pub fn match_locations<'a>(locations: &'a [Location], place: &str) -> Vec<&'a Location> {
    let trimmed = place.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.eq_ignore_ascii_case("all") {
        return locations.iter().collect();
    }
    if let Some((prov, city)) = trimmed.split_once('/') {
        let prov = prov.trim();
        let city = city.trim();
        if prov.is_empty() || city.is_empty() || city.contains('/') {
            return Vec::new();
        }
        if city.eq_ignore_ascii_case("all") || city == "*" {
            return locations
                .iter()
                .filter(|loc| names_match(&loc.province, prov))
                .collect();
        }
        return locations
            .iter()
            .filter(|loc| names_match(&loc.province, prov) && names_match(&loc.city, city))
            .collect();
    }
    let by_province: Vec<&Location> = locations
        .iter()
        .filter(|loc| names_match(&loc.province, trimmed))
        .collect();
    if !by_province.is_empty() {
        return by_province;
    }
    let by_id: Vec<&Location> = locations
        .iter()
        .filter(|loc| loc.id.eq_ignore_ascii_case(trimmed))
        .collect();
    if !by_id.is_empty() {
        return by_id;
    }
    locations
        .iter()
        .filter(|loc| names_match(&loc.city, trimmed))
        .collect()
}

fn append_location_slots(
    slots: &mut Vec<TargetSlot>,
    seen: &mut BTreeSet<(String, Operator)>,
    loc: &Location,
    ops: &[Operator],
) {
    for &op in ops {
        let candidates: Vec<(Ipv4Addr, u16)> = loc
            .nodes
            .iter()
            .filter(|n| n.operator == op)
            .map(|n| (n.ip, n.port))
            .collect();
        if !candidates.is_empty() && seen.insert((loc.id.clone(), op)) {
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

fn names_match(candidate: &str, query: &str) -> bool {
    if candidate.eq_ignore_ascii_case(query) {
        return true;
    }
    let query_slug = slug(query);
    !query_slug.is_empty() && slug(candidate) == query_slug
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
        .filter_map(|choice| match choice {
            OperatorChoice::Telecom => Some(Operator::Telecom),
            OperatorChoice::Unicom => Some(Operator::Unicom),
            OperatorChoice::Mobile => Some(Operator::Mobile),
            OperatorChoice::Cernet => Some(Operator::Cernet),
            OperatorChoice::Cbn => Some(Operator::Cbn),
            OperatorChoice::Other => Some(Operator::Other),
            OperatorChoice::All => None,
        })
        .collect();
    Operator::ALL
        .into_iter()
        .filter(|op| mapped.contains(op))
        .collect()
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
