//! The nearest cache servers of YouTube and Netflix, from the answers that the program fetched for
//! the item over each family: Google's `report_mapping`, whose line names the id of the server
//! that serves the host, and fast.com's API, whose targets name the servers of Netflix that would
//! serve it. Of an answer, only the server's name and place are kept; the host's own address,
//! which both name, is not.

use serde::Deserialize;
use serde::Serialize;

use super::lists;

/// The longest id of a YouTube cache server, in characters.
pub const LONGEST_ID: usize = 40;

/// The longest city or country of a Netflix cache server, in characters.
pub const LONGEST_PLACE: usize = 40;

/// The code that writes a server's id in its host name: each pair is a character of the name, then
/// the character of the id that it stands for, as `q` stands for `d` in `q0cedn7s`, which is
/// `dub16s03`. Letters and digits only; a `-` stays as it is.
const CODES: [(char, char); 36] = [
    ('0', 'u'),
    ('1', 'z'),
    ('2', 'p'),
    ('3', 'k'),
    ('4', 'f'),
    ('5', 'a'),
    ('6', '5'),
    ('7', '0'),
    ('8', 'v'),
    ('9', 'q'),
    ('a', 'l'),
    ('b', 'g'),
    ('c', 'b'),
    ('d', '6'),
    ('e', '1'),
    ('f', 'w'),
    ('g', 'r'),
    ('h', 'm'),
    ('i', 'h'),
    ('j', 'c'),
    ('k', '7'),
    ('l', '2'),
    ('m', 'x'),
    ('n', 's'),
    ('o', 'n'),
    ('p', 'i'),
    ('q', 'd'),
    ('r', '8'),
    ('s', '3'),
    ('t', 'y'),
    ('u', 't'),
    ('v', 'o'),
    ('w', 'j'),
    ('x', 'e'),
    ('y', '9'),
    ('z', '4'),
];

/// Why an answer names no cache server that the item can trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LookupError {
    /// Google's answer names no server: it is not text, or has no id after `=>`.
    NoServer,
    /// Google's answer names a server whose id the code cannot write: longer than
    /// [`LONGEST_ID`], or with a character other than a lowercase letter, a digit or `-`.
    Id,
    /// fast.com's answer is not the JSON of its API.
    Format,
    /// fast.com's answer names no target.
    NoTarget,
    /// The first target's address is not an HTTPS address of a DNS name or an IP address.
    Url,
    /// The first target's city or country is not 1 to [`LONGEST_PLACE`] characters that print.
    Place,
}

/// The host name of the YouTube cache server that Google's `report_mapping` names, such as
/// `r1---sn-npoldn7s.googlevideo.com` for `192.0.2.1 => sin26s03 (192.0.2.0/24)`.
///
/// # Errors
///
/// When the answer names no server, or one whose id the code cannot write.
pub fn youtube(answer: &[u8]) -> Result<String, LookupError> {
    let text = std::str::from_utf8(answer).map_err(|_| LookupError::NoServer)?;
    let (_, after) = text.split_once("=>").ok_or(LookupError::NoServer)?;
    let id = after
        .split_whitespace()
        .next()
        .ok_or(LookupError::NoServer)?;
    if id.len() > LONGEST_ID {
        return Err(LookupError::Id);
    }
    let code: Option<String> = id
        .chars()
        .map(|plain| {
            if plain == '-' {
                return Some('-');
            }
            CODES
                .iter()
                .find(|(_, stands_for)| *stands_for == plain)
                .map(|(written, _)| *written)
        })
        .collect();
    let code = code.ok_or(LookupError::Id)?;
    Ok(format!("r1---sn-{code}.googlevideo.com"))
}

/// A Netflix cache server that fast.com's API names: its host name and place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    /// Its host name, or its address, such as `ipv4-c001-fra002-ix.1.oca.nflxvideo.net`.
    pub host: String,
    /// Its city, as the API names it.
    pub city: String,
    /// Its country, as the API names it, such as `DE`.
    pub country: String,
}

/// fast.com's answer, of which only the targets are read.
#[derive(Deserialize)]
struct Answer {
    targets: Vec<Target>,
}

/// A target of fast.com's answer.
#[derive(Deserialize)]
struct Target {
    url: String,
    location: Location,
}

/// A target's place.
#[derive(Deserialize)]
struct Location {
    city: String,
    country: String,
}

/// The Netflix cache server of the first target that fast.com's API names: the program fetched
/// the answer over one family, so its targets are servers of that family.
///
/// # Errors
///
/// When the answer is not the API's JSON, names no target, or its first target's address or
/// place cannot be used.
pub fn netflix(answer: &[u8]) -> Result<Cache, LookupError> {
    let answer: Answer = serde_json::from_slice(answer).map_err(|_| LookupError::Format)?;
    let first = answer
        .targets
        .into_iter()
        .next()
        .ok_or(LookupError::NoTarget)?;
    let host = url_host(&first.url).ok_or(LookupError::Url)?;
    let Location { city, country } = first.location;
    if !lists::printable(&city, LONGEST_PLACE) || !lists::printable(&country, LONGEST_PLACE) {
        return Err(LookupError::Place);
    }
    Ok(Cache {
        host,
        city,
        country,
    })
}

/// The host of an HTTPS address: a DNS name or an IP address, an IPv6 address without its
/// brackets.
fn url_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let host = match rest.strip_prefix('[') {
        Some(bracketed) => bracketed.split_once(']')?.0,
        None => rest.split(['/', '?', '#', ':']).next()?,
    };
    lists::host(host).then(|| host.to_owned())
}
