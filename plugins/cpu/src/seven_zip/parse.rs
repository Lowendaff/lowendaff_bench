//! Parser for `7zz b` console output (`Avr:` and `Tot:` rows, plus header version).

use bench_tool::job;

use super::result::Fault;

/// Parsed values of one `7zz b` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// Version from the banner, such as `26.06`.
    pub version: Option<String>,
    /// Compression speed in KiB/s.
    pub compress_kib_s: u64,
    /// Compression rating in MIPS.
    pub compress_mips: u64,
    /// Decompression speed in KiB/s.
    pub decompress_kib_s: u64,
    /// Decompression rating in MIPS.
    pub decompress_mips: u64,
    /// Combined CPU usage percentage.
    pub usage_pct: u64,
    /// Combined R/U rating in MIPS.
    pub ru_mips: u64,
    /// Combined total rating in MIPS.
    pub total_mips: u64,
}

/// Parses one finished `7zz b` output.
pub fn reading(output: &job::Output) -> Result<Reading, Fault> {
    if let Some(fault) = output.fault() {
        return Err(Fault::Tool(fault));
    }
    let text = &output.text;
    let (compress_kib_s, compress_mips, decompress_kib_s, decompress_mips) =
        parse_avr(text).ok_or(Fault::NoScores)?;
    let (usage_pct, ru_mips, total_mips) = parse_tot(text).ok_or(Fault::NoScores)?;
    Ok(Reading {
        version: version(text),
        compress_kib_s,
        compress_mips,
        decompress_kib_s,
        decompress_mips,
        usage_pct,
        ru_mips,
        total_mips,
    })
}

/// Parses `Avr:     27858   146  17026  24818  |     139872   199   5681  11311`.
fn parse_avr(text: &str) -> Option<(u64, u64, u64, u64)> {
    let rest = text
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("Avr:"))?;
    let (left, right) = rest.split_once('|')?;
    let comp: Vec<u64> = left
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let decomp: Vec<u64> = right
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if comp.len() != 4 || decomp.len() != 4 || comp[3] == 0 || decomp[3] == 0 {
        return None;
    }
    Some((comp[0], comp[3], decomp[0], decomp[3]))
}

/// Parses `Tot:             172  11353  18065`.
fn parse_tot(text: &str) -> Option<(u64, u64, u64)> {
    let rest = text
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("Tot:"))?;
    let cols: Vec<u64> = rest
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if cols.len() != 3 || cols[2] == 0 {
        return None;
    }
    Some((cols[0], cols[1], cols[2]))
}

/// Extracts `26.06` from `7-Zip (z) 26.06 (arm64) : ...` or `7-Zip 26.06 ...`.
fn version(text: &str) -> Option<String> {
    for line in text.lines().take(5) {
        let Some(after) = line.trim().strip_prefix("7-Zip") else {
            continue;
        };
        for token in after.split_whitespace() {
            let plain = token.starts_with(|c: char| c.is_ascii_digit())
                && token.chars().all(|c| c.is_ascii_digit() || c == '.');
            if plain {
                return Some(token.to_owned());
            }
        }
    }
    None
}
