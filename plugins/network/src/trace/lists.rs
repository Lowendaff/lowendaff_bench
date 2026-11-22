//! What the item's two lists share, the targets in this repository and Netflix's exchange ports:
//! their size, the version of their format, and how a list breaks a rule. A field that the item
//! does not know is ignored, so that a newer list still serves this program; a newer version of
//! the format is not read.

use std::net::IpAddr;

use serde::Deserialize;
use serde::Serialize;

/// The version of the format that this program reads, of either list.
pub const VERSION: u64 = 1;

/// The largest list, in bytes: the most that the program downloads for a plugin's file.
pub const LIMIT: usize = 64 * 1024;

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
    /// It holds more entries than a list may: the number that it holds.
    TooMany(usize),
    /// An entry breaks a rule: its position in the list, from 1, and the rule.
    Entry(usize, Rule),
    /// Two entries have this id, or this address.
    Duplicate(String),
}

/// A rule of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Rule {
    /// A target's id: 1 to 40 characters of `a` to `z`, digits and `-`, other than the ids of
    /// the cache servers, `youtube` and `netflix`.
    Id,
    /// A target's group: `china` or `international`.
    Group,
    /// A target's name: 1 to 40 characters that print.
    Name,
    /// A target's AS number: from 1 to 4294967295.
    Asn,
    /// A target's IPv4 name: a DNS name or an IPv4 address.
    Ipv4,
    /// A target's IPv6 name: a DNS name or an IPv6 address.
    Ipv6,
    /// A port's address: an IPv4 address, or an IPv6 address without an IPv4 part, in its
    /// canonical form (RFC 5952): in lowercase, without leading zeros, its longest run of zeros
    /// cut short.
    Address,
    /// A port's exchange: 1 to 60 characters that print.
    Exchange,
}

/// Checks the size and the version of a list, before anything else of it is read, so that a
/// newer list that this program cannot read says so.
///
/// # Errors
///
/// When the list is too large, not JSON with a version, or of another version.
pub fn versioned(bytes: &[u8]) -> Result<(), ListError> {
    #[derive(Deserialize)]
    struct Versioned {
        version: u64,
    }
    if bytes.len() > LIMIT {
        return Err(ListError::TooLarge);
    }
    let versioned: Versioned =
        serde_json::from_slice(bytes).map_err(|err| ListError::Format(err.to_string()))?;
    match versioned.version {
        VERSION => Ok(()),
        newer if newer > VERSION => Err(ListError::Newer(newer)),
        other => Err(ListError::Version(other)),
    }
}

/// 1 to `longest` characters, none of them a control character, so that a list cannot write
/// control sequences to the terminal.
#[must_use]
pub fn printable(text: &str, longest: usize) -> bool {
    (1..=longest).contains(&text.chars().count()) && !text.chars().any(char::is_control)
}

/// An IP address, or a DNS name: labels of 1 to 63 letters, digits and `-`, none at either end
/// of a label, 253 characters at most. So a name never starts with `-`, and NextTrace cannot take
/// it for an option.
#[must_use]
pub fn host(host: &str) -> bool {
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

/// Whether `host` is permitted in an unpinned remote list or cache-lookup response: rejects
/// `localhost`, `*.localhost`, and non-public IP literals (loopback, private, link-local, metadata,
/// multicast, and non-global IPv6).
#[must_use]
pub fn is_remote_safe_host(host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return crate::endpoint::is_routable_ip(ip);
    }
    let lower = host.to_ascii_lowercase();
    lower != "localhost" && !lower.ends_with(".localhost")
}
