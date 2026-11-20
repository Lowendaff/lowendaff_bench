//! Default `/tmp` report archive paths (`lowendaff-bench-<YYYYMMDD-HHMMSS>.<ext>`) and file saving.
//! Benchmark working directories and helper binaries never use `/tmp` (`work_dir` defaults to `.`),
//! whereas finished plain-text, Markdown, and JSON reports are safe to archive in `/tmp`.

use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::app;
use crate::locale;
use crate::report::Format;

/// Default directory where finished benchmark reports are archived automatically.
pub const DEFAULT_REPORT_DIR: &str = "/tmp";

/// File extension corresponding to `format`.
#[must_use]
pub const fn extension(format: Format) -> &'static str {
    match format {
        Format::Text => "txt",
        Format::Markdown => "md",
        Format::Json => "json",
    }
}

/// Formats `unix_secs` as proleptic Gregorian UTC `YYYYMMDD-HHMMSS`.
#[must_use]
pub fn format_utc_timestamp(unix_secs: u64) -> String {
    let days = unix_secs / 86_400;
    let day_secs = unix_secs % 86_400;
    let hour = day_secs / 3_600;
    let minute = (day_secs % 3_600) / 60;
    let second = day_secs % 60;

    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Builds `lowendaff-bench-<YYYYMMDD-HHMMSS>.<ext>` inside `dir`.
#[must_use]
pub fn path_in(dir: &Path, unix_secs: u64, format: Format) -> PathBuf {
    let stamp = format_utc_timestamp(unix_secs);
    let ext = extension(format);
    dir.join(format!("lowendaff-bench-{stamp}.{ext}"))
}

/// Builds the default `/tmp/lowendaff-bench-<YYYYMMDD-HHMMSS>.<ext>` path for the current wall
/// clock.
#[must_use]
pub fn default_path(format: Format) -> PathBuf {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |dur| dur.as_secs());
    path_in(Path::new(DEFAULT_REPORT_DIR), secs, format)
}

/// Human-readable placeholder shown in the TUI when automatic `/tmp` report saving is active.
#[must_use]
pub fn auto_label(format: Format) -> String {
    format!(
        "{DEFAULT_REPORT_DIR}/lowendaff-bench-<timestamp>.{} (auto)",
        extension(format)
    )
}

/// Writes `contents` to `path`, returning a localized error string on failure.
pub fn save_copy(path: &Path, contents: &str) -> Result<(), String> {
    let mut file = app::create_output(path)?;
    file.write_all(contents.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|err| locale::words().output_unwritable(path, &err))
}
