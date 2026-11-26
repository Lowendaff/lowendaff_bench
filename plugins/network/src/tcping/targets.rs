//! Parser, schema validator, and selector for `servers/tcping.json`.
//!
//! Enforces 1-to-1 parity with `scripts/tcping-targets.sh` in `Lowendaff/bench_data` and the shared
//! fixture suite in `tests/fixtures/tcping-lists/`.

use std::net::IpAddr;

use bench_api::IpFamily;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

use super::result::Group;
use super::result::GroupChoice;
use super::result::Operator;
use crate::trace;

/// Supported `servers/tcping.json` schema version.
pub const VERSION: u64 = 1;

/// Maximum size of `servers/tcping.json` (`128 KiB`).
pub const LIMIT: usize = 128 * 1024;

/// Maximum targets permitted in `servers/tcping.json`.
pub const MOST_TARGETS: usize = 300;

/// Maximum length of `id`, `region`, `city`, and `name`.
const LONGEST_TEXT: usize = 40;

/// Maximum candidate endpoints (`primary` + `backup`) per IP family on one target.
const MOST_ENDPOINTS: usize = 2;

/// One candidate TCP endpoint (`primary` at index `0`, optional `backup` at index `1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Endpoint {
    /// DNS hostname or IP literal matching its family.
    pub host: String,
    /// TCP port (`1..=65535`).
    pub port: u16,
}

/// One validated TCP handshake latency target from `servers/tcping.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Target {
    /// Unique lowercase identifier (`1..=40` chars of `a-z`, `0-9`, `-`, not `all`).
    pub id: String,
    /// Target group (`china`, `china_premium`, or `global`).
    pub group: Group,
    /// Province or world region (`1..=40` printable ASCII characters).
    pub region: String,
    /// City name (`1..=40` printable ASCII characters).
    pub city: String,
    /// Display name (`1..=40` printable ASCII characters).
    pub name: String,
    /// Carrier classification.
    pub operator: Operator,
    /// Autonomous system number (`1..=4_294_967_295`).
    pub asn: u32,
    /// Whether the target runs by default when `--tcping-all` is not set.
    pub default: bool,
    /// Up to two IPv4 endpoints (`primary` and optional `backup`).
    pub ipv4: Vec<Endpoint>,
    /// Up to two IPv6 endpoints (`primary` and optional `backup`).
    pub ipv6: Vec<Endpoint>,
}

impl Target {
    /// Candidate endpoints for `family` (`0..=2`).
    #[must_use]
    pub fn endpoints(&self, family: IpFamily) -> &[Endpoint] {
        match family {
            IpFamily::Ipv4 => &self.ipv4,
            IpFamily::Ipv6 => &self.ipv6,
        }
    }
}

/// Broken field rule on a single target entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Rule {
    /// Invalid or reserved `id`.
    Id,
    /// Unknown `group`.
    Group,
    /// Empty, overlong, or non-printable-ASCII `region`.
    Region,
    /// Empty, overlong, or non-printable-ASCII `city`.
    City,
    /// Empty, overlong, or non-printable-ASCII `name`.
    Name,
    /// Unknown `operator`.
    Operator,
    /// Non-integer or out-of-range `asn`.
    Asn,
    /// Non-boolean `default`.
    Default,
    /// Neither `ipv4` nor `ipv6` provided.
    Endpoints,
    /// Malformed, out-of-family, non-public, or duplicate `ipv4` endpoint list.
    Ipv4,
    /// Malformed, out-of-family, non-public, or duplicate `ipv6` endpoint list.
    Ipv6,
}

impl Rule {
    /// Field name reported in [`super::result::Failure::InvalidEntry`].
    #[must_use]
    pub const fn as_field(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Group => "group",
            Self::Region => "region",
            Self::City => "city",
            Self::Name => "name",
            Self::Operator => "operator",
            Self::Asn => "asn",
            Self::Default => "default",
            Self::Endpoints => "endpoints",
            Self::Ipv4 => "ipv4",
            Self::Ipv6 => "ipv6",
        }
    }
}

/// Validation failure when reading `servers/tcping.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ListError {
    /// File size exceeds [`LIMIT`] (`128 KiB`).
    TooLarge,
    /// Input is not valid JSON or missing top-level object/array shape.
    Format(String),
    /// `version` is not [`VERSION`].
    Version(u64),
    /// `targets` is empty or has more than [`MOST_TARGETS`] entries.
    Count(usize),
    /// Target at 1-based position broke `Rule`.
    Entry(usize, Rule),
    /// Two targets carry the same `id`.
    Duplicate(String),
}

/// Parses and validates `bytes` allowing local/loopback IP literals (for fixtures and local file
/// overrides).
///
/// # Errors
///
/// Returns [`ListError`] when `bytes` violates size, JSON structure, version, or target rules.
pub fn parse(bytes: &[u8]) -> Result<Vec<Target>, ListError> {
    parse_for_source(bytes, true)
}

/// Parses and validates `bytes`, enforcing public routable endpoints when `allow_local` is `false`.
///
/// # Errors
///
/// Returns [`ListError`] when `bytes` violates size, JSON structure, version, or target rules.
pub fn parse_for_source(bytes: &[u8], allow_local: bool) -> Result<Vec<Target>, ListError> {
    if bytes.len() > LIMIT {
        return Err(ListError::TooLarge);
    }
    let root: Value =
        serde_json::from_slice(bytes).map_err(|err| ListError::Format(err.to_string()))?;
    let Some(obj) = root.as_object() else {
        return Err(ListError::Format(
            "top-level value is not an object".to_owned(),
        ));
    };
    let Some(ver_val) = obj.get("version") else {
        return Err(ListError::Format("missing version".to_owned()));
    };
    let Some(ver) = ver_val.as_u64() else {
        return Err(ListError::Format(
            "version must be an unsigned integer".to_owned(),
        ));
    };
    if ver != VERSION {
        return Err(ListError::Version(ver));
    }
    let Some(raw_targets) = obj.get("targets").and_then(Value::as_array) else {
        return Err(ListError::Format("missing targets array".to_owned()));
    };
    if raw_targets.is_empty() || raw_targets.len() > MOST_TARGETS {
        return Err(ListError::Count(raw_targets.len()));
    }

    let mut parsed = Vec::with_capacity(raw_targets.len());
    for (idx, item) in raw_targets.iter().enumerate() {
        let Some(entry_obj) = item.as_object() else {
            return Err(ListError::Format(format!(
                "target {} is not an object",
                idx + 1
            )));
        };
        let target =
            parse_entry(entry_obj, allow_local).map_err(|rule| ListError::Entry(idx + 1, rule))?;
        if parsed
            .iter()
            .any(|existing: &Target| existing.id == target.id)
        {
            return Err(ListError::Duplicate(target.id));
        }
        parsed.push(target);
    }
    Ok(parsed)
}

fn parse_entry(obj: &Map<String, Value>, allow_local: bool) -> Result<Target, Rule> {
    let id = obj
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| valid_id(s))
        .ok_or(Rule::Id)?;
    let group = match obj.get("group").and_then(Value::as_str) {
        Some("china") => Group::China,
        Some("china_premium") => Group::ChinaPremium,
        Some("global") => Group::Global,
        _ => return Err(Rule::Group),
    };
    let region = obj
        .get("region")
        .and_then(Value::as_str)
        .filter(|s| ascii_printable(s, LONGEST_TEXT))
        .ok_or(Rule::Region)?;
    let city = obj
        .get("city")
        .and_then(Value::as_str)
        .filter(|s| ascii_printable(s, LONGEST_TEXT))
        .ok_or(Rule::City)?;
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| ascii_printable(s, LONGEST_TEXT))
        .ok_or(Rule::Name)?;
    let operator = match obj.get("operator").and_then(Value::as_str) {
        Some("telecom") => Operator::Telecom,
        Some("unicom") => Operator::Unicom,
        Some("mobile") => Operator::Mobile,
        Some("cernet") => Operator::Cernet,
        Some("cbn") => Operator::Cbn,
        Some("other") => Operator::Other,
        _ => return Err(Rule::Operator),
    };
    let asn = obj
        .get("asn")
        .and_then(strict_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&n| n >= 1)
        .ok_or(Rule::Asn)?;
    let default = match obj.get("default") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(_) => return Err(Rule::Default),
    };

    let has_v4 = matches!(obj.get("ipv4"), Some(v) if !v.is_null());
    let has_v6 = matches!(obj.get("ipv6"), Some(v) if !v.is_null());
    if !has_v4 && !has_v6 {
        return Err(Rule::Endpoints);
    }
    let ipv4 = match obj.get("ipv4") {
        None | Some(Value::Null) => Vec::new(),
        Some(v) => parse_endpoints(v, false, allow_local).ok_or(Rule::Ipv4)?,
    };
    let ipv6 = match obj.get("ipv6") {
        None | Some(Value::Null) => Vec::new(),
        Some(v) => parse_endpoints(v, true, allow_local).ok_or(Rule::Ipv6)?,
    };

    Ok(Target {
        id: id.to_owned(),
        group,
        region: region.to_owned(),
        city: city.to_owned(),
        name: name.to_owned(),
        operator,
        asn,
        default,
        ipv4,
        ipv6,
    })
}

fn parse_endpoints(val: &Value, ipv6: bool, allow_local: bool) -> Option<Vec<Endpoint>> {
    let arr = val.as_array()?;
    if !(1..=MOST_ENDPOINTS).contains(&arr.len()) {
        return None;
    }
    let mut out: Vec<Endpoint> = Vec::with_capacity(arr.len());
    for item in arr {
        let ep_obj = item.as_object()?;
        let host = ep_obj.get("host")?.as_str()?;
        if !valid_endpoint_host(host, ipv6, allow_local) {
            return None;
        }
        let raw_port = strict_u64(ep_obj.get("port")?)?;
        let port = u16::try_from(raw_port).ok().filter(|&p| p >= 1)?;
        if out
            .iter()
            .any(|prev| prev.port == port && prev.host.eq_ignore_ascii_case(host))
        {
            return None;
        }
        out.push(Endpoint {
            host: host.to_owned(),
            port,
        });
    }
    Some(out)
}

fn strict_u64(val: &Value) -> Option<u64> {
    let NumberText(ok) = match val {
        Value::Number(num) => NumberText(num.as_u64()),
        _ => NumberText(None),
    };
    ok
}

struct NumberText(Option<u64>);

fn valid_id(id: &str) -> bool {
    (1..=LONGEST_TEXT).contains(&id.len())
        && id != "all"
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn ascii_printable(text: &str, longest: usize) -> bool {
    (1..=longest).contains(&text.len()) && text.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

fn valid_endpoint_host(host: &str, ipv6: bool, allow_local: bool) -> bool {
    if ipv6 && let Ok(IpAddr::V6(v6)) = host.parse::<IpAddr>() {
        // Enforce RFC 5952 canonical lowercase IPv6 without IPv4-embedded dots, matching
        // list-rules.jq.
        if host != v6.to_string() || host.contains('.') {
            return false;
        }
        return allow_local || bench_api::is_routable_endpoint_ip(IpAddr::V6(v6));
    }
    match host.parse::<IpAddr>() {
        Ok(addr) => {
            !ipv6
                && addr.is_ipv4()
                && is_canonical_v4(host)
                && (allow_local || bench_api::is_routable_endpoint_ip(addr))
        }
        Err(_) => {
            !host.contains(':')
                && trace::lists::host(host)
                && (allow_local || trace::lists::is_remote_safe_host(host))
        }
    }
}

fn is_canonical_v4(host: &str) -> bool {
    let Ok(v4) = host.parse::<std::net::Ipv4Addr>() else {
        return false;
    };
    host == v4.to_string()
}

/// Filters `catalog` by `groups` and `all_targets`, preserving `(china, china_premium, global)`
/// order and catalog order within each group.
#[must_use]
pub fn choose(catalog: &[Target], groups: &[GroupChoice], all_targets: bool) -> Vec<Target> {
    let include_all_groups = groups.is_empty() || groups.contains(&GroupChoice::All);
    let wants_group = |group: Group| {
        if include_all_groups {
            return true;
        }
        match group {
            Group::China => groups.contains(&GroupChoice::China),
            Group::ChinaPremium => groups.contains(&GroupChoice::ChinaPremium),
            Group::Global => groups.contains(&GroupChoice::Global),
        }
    };

    let mut chosen = Vec::new();
    for group in [Group::China, Group::ChinaPremium, Group::Global] {
        if !wants_group(group) {
            continue;
        }
        for target in catalog {
            if target.group == group && (all_targets || target.default) {
                chosen.push(target.clone());
            }
        }
    }
    chosen
}
