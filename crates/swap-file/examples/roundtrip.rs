//! Turns a 64 MiB swap file on and off again, for the CI step "Swap round trip" (design cpu,
//! §9.3). It needs root on Linux and a filesystem with swap files, such as ext4.
//!
//! Usage: roundtrip <directory>

use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use swap_file::SwapFile;

fn main() -> ExitCode {
    match round_trip() {
        Ok(path) => {
            println!(
                "swap round trip: {} was on, then off and removed",
                path.display()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("swap round trip: {err}");
            ExitCode::FAILURE
        }
    }
}

fn round_trip() -> Result<PathBuf, String> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: roundtrip <directory>")?;
    // /proc/swaps shows the canonical path.
    let dir = std::fs::canonicalize(&dir).map_err(|err| format!("{dir}: {err}"))?;
    let path = dir.join("swapfile");
    let swap = SwapFile::try_new(&path, 64 << 20, &|| false)
        .map_err(|err| format!("turning on {}: {err}", path.display()))?;
    if !listed(&path)? {
        return Err(format!("{} is on but not in /proc/swaps", path.display()));
    }
    swap.remove()
        .map_err(|err| format!("turning off {}: {err}", path.display()))?;
    if listed(&path)? {
        return Err(format!("{} is still in /proc/swaps", path.display()));
    }
    if path.exists() {
        return Err(format!("{} was not removed", path.display()));
    }
    Ok(path)
}

/// Whether `/proc/swaps` lists `path`.
fn listed(path: &Path) -> Result<bool, String> {
    let swaps =
        std::fs::read_to_string("/proc/swaps").map_err(|err| format!("/proc/swaps: {err}"))?;
    Ok(swaps
        .lines()
        .any(|line| line.split_whitespace().next() == path.to_str()))
}
