//! Netflix's exchange ports, from PeeringDB: the addresses that Netflix's network takes at the
//! internet exchanges where it peers, so that a hop at such an address is named with its
//! exchange. The list is a file of this repository, which the scheduled data workflow keeps
//! current, in the format and under the rules that this module checks.

use std::collections::BTreeMap;
use std::net::IpAddr;

use serde::Deserialize;

use super::lists;
use super::lists::ListError;
use super::lists::Rule;

/// The most ports that a list may hold: more than twice the ports that Netflix had in 2026, and
/// few enough for a list within [`lists::LIMIT`], which could not hold 2,000.
pub const MOST_PORTS: usize = 1000;

/// The longest name of an exchange, in characters.
pub const LONGEST_EXCHANGE: usize = 60;

/// The exchange of each port, by its address.
pub type Exchanges = BTreeMap<IpAddr, String>;

/// The list as it is written, after its version.
#[derive(Deserialize)]
struct Written {
    ports: Vec<WrittenPort>,
}

/// A port as it is written.
#[derive(Deserialize)]
struct WrittenPort {
    address: String,
    exchange: String,
}

/// Reads a list, and checks every rule.
///
/// # Errors
///
/// When the list is too large, not of the format, of another version, or breaks a rule.
pub fn parse(bytes: &[u8]) -> Result<Exchanges, ListError> {
    lists::versioned(bytes)?;
    let written: Written =
        serde_json::from_slice(bytes).map_err(|err| ListError::Format(err.to_string()))?;
    if written.ports.len() > MOST_PORTS {
        return Err(ListError::TooMany(written.ports.len()));
    }
    let mut exchanges = Exchanges::new();
    for (index, port) in written.ports.into_iter().enumerate() {
        let entry = |rule| ListError::Entry(index + 1, rule);
        let address: IpAddr = port.address.parse().map_err(|_| entry(Rule::Address))?;
        // Written as the address prints, so that the checking script, which compares addresses as
        // they are written, finds the same duplicates.
        let canonical = address.to_string();
        if canonical != port.address || canonical.contains('.') && address.is_ipv6() {
            return Err(entry(Rule::Address));
        }
        if !lists::printable(&port.exchange, LONGEST_EXCHANGE) {
            return Err(entry(Rule::Exchange));
        }
        if exchanges.insert(address, port.exchange).is_some() {
            return Err(ListError::Duplicate(address.to_string()));
        }
    }
    Ok(exchanges)
}
