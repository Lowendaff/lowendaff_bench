//! The server list, the one in this repository or a file in its format: what it holds, and the
//! rules that the item checks before it uses a list, as CI checks them in every pull request that
//! changes the repository's. A field that the item does not know is ignored, so that a newer list
//! still serves this program; a newer version of the format is not read.

use std::net::IpAddr;

use serde::Deserialize;
use serde::Serialize;

/// The version of the format that this program reads.
pub const VERSION: u64 = 1;

/// The largest list, in bytes: a list is a page of text, not a download.
pub const LIMIT: usize = 64 * 1024;

/// The most servers that a list may hold.
pub const MOST_SERVERS: usize = 100;

/// The most ports that a server may offer.
pub const MOST_PORTS: u32 = 64;

/// The longest place, provider or link, in characters.
pub const LONGEST_TEXT: usize = 40;

/// The longest id, in characters.
pub const LONGEST_ID: usize = 40;

/// A server of the list, checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Server {
    /// Its id, unique in the list, such as `tokyo`.
    pub id: String,
    /// Its country's ISO 3166 code, such as `JP`.
    pub country: String,
    /// Its place, such as `Tokyo`.
    pub location: String,
    /// Who runs it, such as `Leaseweb`.
    pub provider: String,
    /// Its DNS name or IP address.
    pub host: String,
    /// The first and the last port of its range; one port when they are the same.
    pub ports: (u16, u16),
    /// Whether it answers over IPv4.
    pub ipv4: bool,
    /// Whether it answers over IPv6.
    pub ipv6: bool,
    /// Its link, as its operator gives it, such as `10G`.
    pub link: String,
    /// Whether the item tests it unless told otherwise.
    pub default: bool,
}

impl Server {
    /// Its ports, in order.
    #[must_use]
    pub fn port_list(&self) -> Vec<u16> {
        (self.ports.0..=self.ports.1).collect()
    }
}

/// What makes a list unusable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ListError {
    /// It is larger than [`LIMIT`] bytes.
    TooLarge,
    /// It is not a list of the format: the JSON parser's reason.
    Format(String),
    /// It is of a newer version of the format than this program reads.
    Newer(u64),
    /// It is of a version of the format that never was.
    Version(u64),
    /// It holds more than [`MOST_SERVERS`] servers.
    TooMany(usize),
    /// A server breaks a rule: the server's position in the list, from 1, and the rule.
    Server(usize, Rule),
    /// Two servers have this id.
    Duplicate(String),
}

/// A rule of a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Rule {
    /// The id: 1 to 40 characters of `a` to `z`, digits and `-`.
    Id,
    /// The country: two capital letters.
    Country,
    /// The host: a DNS name or an IP address.
    Host,
    /// The ports: one, or a range, from 1024 to 65535, at most [`MOST_PORTS`] of them.
    Ports,
    /// The families: `ipv4`, `ipv6` or both, each once.
    Families,
    /// The place, the provider or the link: 1 to 40 characters that print.
    Text,
}

/// The list as it is written, after its version.
#[derive(Deserialize)]
struct Written {
    servers: Vec<WrittenServer>,
}

/// A server as it is written.
#[derive(Deserialize)]
struct WrittenServer {
    id: String,
    country: String,
    location: String,
    provider: String,
    host: String,
    ports: String,
    families: Vec<String>,
    link: String,
    #[serde(default)]
    default: bool,
}

/// Reads a list, and checks every rule.
///
/// # Errors
///
/// When the list is too large, not of the format, of another version, or breaks a rule.
pub fn parse(bytes: &[u8]) -> Result<Vec<Server>, ListError> {
    if bytes.len() > LIMIT {
        return Err(ListError::TooLarge);
    }
    // The version first, so that a newer list that this program cannot read says so.
    #[derive(Deserialize)]
    struct Versioned {
        version: u64,
    }
    let versioned: Versioned =
        serde_json::from_slice(bytes).map_err(|err| ListError::Format(err.to_string()))?;
    match versioned.version {
        VERSION => {}
        newer if newer > VERSION => return Err(ListError::Newer(newer)),
        other => return Err(ListError::Version(other)),
    }
    let written: Written =
        serde_json::from_slice(bytes).map_err(|err| ListError::Format(err.to_string()))?;
    if written.servers.len() > MOST_SERVERS {
        return Err(ListError::TooMany(written.servers.len()));
    }
    let mut servers: Vec<Server> = Vec::with_capacity(written.servers.len());
    for (index, server) in written.servers.into_iter().enumerate() {
        let server = check(server).map_err(|rule| ListError::Server(index + 1, rule))?;
        if servers.iter().any(|known| known.id == server.id) {
            return Err(ListError::Duplicate(server.id));
        }
        servers.push(server);
    }
    Ok(servers)
}

/// A written server, checked against every rule.
fn check(server: WrittenServer) -> Result<Server, Rule> {
    if !id(&server.id) {
        return Err(Rule::Id);
    }
    if !(server.country.len() == 2 && server.country.bytes().all(|byte| byte.is_ascii_uppercase()))
    {
        return Err(Rule::Country);
    }
    if !host(&server.host) {
        return Err(Rule::Host);
    }
    let ports = ports(&server.ports).ok_or(Rule::Ports)?;
    let (ipv4, ipv6) = families(&server.families).ok_or(Rule::Families)?;
    if ![&server.location, &server.provider, &server.link]
        .iter()
        .all(|text| printable(text))
    {
        return Err(Rule::Text);
    }
    Ok(Server {
        id: server.id,
        country: server.country,
        location: server.location,
        provider: server.provider,
        host: server.host,
        ports,
        ipv4,
        ipv6,
        link: server.link,
        default: server.default,
    })
}

/// 1 to 40 characters of `a` to `z`, digits and `-`.
fn id(id: &str) -> bool {
    (1..=LONGEST_ID).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// An IP address, or a DNS name: labels of 1 to 63 letters, digits and `-`, none at either end
/// of a label, 253 characters at most. So a host never starts with `-`, and iperf3 cannot take it
/// for an option.
fn host(host: &str) -> bool {
    if host.parse::<IpAddr>().is_ok() {
        return true;
    }
    let label = |label: &str| {
        (1..=63).contains(&label.len())
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    };
    (1..=253).contains(&host.len()) && host.split('.').all(label)
}

/// `5201` or `5201-5210`: from 1024 to 65535, the first not after the last, at most
/// [`MOST_PORTS`] of them.
fn ports(ports: &str) -> Option<(u16, u16)> {
    let (first, last) = ports.split_once('-').unwrap_or((ports, ports));
    let number = |text: &str| -> Option<u16> {
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        text.parse().ok()
    };
    let (first, last) = (number(first)?, number(last)?);
    let count = u32::from(last).checked_sub(u32::from(first))? + 1;
    (first >= 1024 && count <= MOST_PORTS).then_some((first, last))
}

/// `ipv4`, `ipv6` or both, each once.
fn families(families: &[String]) -> Option<(bool, bool)> {
    let ipv4 = families.iter().filter(|family| *family == "ipv4").count();
    let ipv6 = families.iter().filter(|family| *family == "ipv6").count();
    let known = ipv4 + ipv6 == families.len() && ipv4 <= 1 && ipv6 <= 1;
    (known && !families.is_empty()).then_some((ipv4 == 1, ipv6 == 1))
}

/// 1 to 40 characters, none of them a control character, so that a list cannot write control
/// sequences to the terminal.
fn printable(text: &str) -> bool {
    (1..=LONGEST_TEXT).contains(&text.chars().count()) && !text.chars().any(char::is_control)
}

/// The servers to test, and what was asked for that no server of the list has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// The servers, in the list's order.
    pub servers: Vec<Server>,
    /// The countries or ids asked for that no server has, as they were given.
    pub unknown: Vec<String>,
}

/// The servers to test: those with the `ids` given, else those of the `countries` given, in any
/// case, or every one for `all`, else the list's defaults.
#[must_use]
pub fn choose(servers: &[Server], countries: &[String], ids: &[String]) -> Chosen {
    if !ids.is_empty() {
        let unknown = ids
            .iter()
            .filter(|wanted| !servers.iter().any(|server| server.id == **wanted))
            .cloned()
            .collect();
        let chosen = servers
            .iter()
            .filter(|server| ids.contains(&server.id))
            .cloned()
            .collect();
        return Chosen {
            servers: chosen,
            unknown,
        };
    }
    if countries
        .iter()
        .any(|country| country.eq_ignore_ascii_case("all"))
    {
        return Chosen {
            servers: servers.to_vec(),
            unknown: Vec::new(),
        };
    }
    if !countries.is_empty() {
        let has = |country: &String| {
            servers
                .iter()
                .any(|server| server.country.eq_ignore_ascii_case(country))
        };
        let unknown = countries
            .iter()
            .filter(|country| !has(country))
            .cloned()
            .collect();
        let chosen = servers
            .iter()
            .filter(|server| {
                countries
                    .iter()
                    .any(|country| server.country.eq_ignore_ascii_case(country))
            })
            .cloned()
            .collect();
        return Chosen {
            servers: chosen,
            unknown,
        };
    }
    Chosen {
        servers: servers
            .iter()
            .filter(|server| server.default)
            .cloned()
            .collect(),
        unknown: Vec::new(),
    }
}
