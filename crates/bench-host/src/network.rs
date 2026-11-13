//! What the network module of the program found out about the host's network: whether each
//! family reaches the Internet, and the public address that outside services see. This crate
//! connects nowhere; it only holds the result.

use std::net::IpAddr;
use std::net::SocketAddr;

/// IPv4 and IPv6, each found out on its own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Network {
    /// The IPv4 side.
    pub ipv4: Family,
    /// The IPv6 side.
    pub ipv6: Family,
}

/// One address family: the reachability targets as they were tried, and the public address.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Family {
    /// Each target tried, in order, with its result; the first that connects ends the list.
    pub targets: Vec<(SocketAddr, Reach)>,
    /// The public address, or why there is none.
    pub address: Address,
}

impl Family {
    /// Whether a handshake with any target completed.
    #[must_use]
    pub fn reachable(&self) -> bool {
        self.targets
            .iter()
            .any(|(_, reach)| *reach == Reach::Connected)
    }
}

/// How a TCP handshake with a target ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// The handshake completed.
    Connected,
    /// No answer within the time allowed.
    TimedOut,
    /// The target refused the connection.
    Refused,
    /// The host has no route to the target.
    NoRoute,
    /// Any other error.
    Failed,
}

/// The public address of one family.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Address {
    /// No source was asked, because the family is unreachable or was not probed.
    #[default]
    NotAsked,
    /// The first valid answer, and the name of the source that gave it.
    Found {
        /// The address, complete; only the output masks it.
        address: IpAddr,
        /// The source's name, such as `Cloudflare`.
        source: &'static str,
    },
    /// Every source failed: each one's name, and why.
    NotFound(Vec<(&'static str, Why)>),
}

/// Why a source gave no address. Never the body of its answer, which may hold the address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// No answer within the time allowed.
    TimedOut,
    /// An HTTP status other than success.
    Status(u16),
    /// The answer is not an address of the family.
    NotAnAddress,
    /// The answer is an address of the family, but not a public one.
    NotPublic,
    /// The source's name has no address of the family.
    NoAddress,
    /// The connection failed otherwise.
    Unreachable,
}
