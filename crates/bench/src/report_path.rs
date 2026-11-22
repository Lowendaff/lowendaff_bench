//! Default `/tmp` report archive paths (`lowendaff-bench-<YYYYMMDD-HHMMSS>.<ext>`) and file saving.
//! Benchmark working directories and helper binaries never use `/tmp` (`work_dir` defaults to `.`),
//! whereas finished plain-text, Markdown, and JSON reports are safe to archive in `/tmp` when
//! created atomically with `O_CREAT | O_EXCL | O_NOFOLLOW` and `0600` permissions.

use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use rustix::fs::Mode;
use rustix::fs::OFlags;

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

/// Atomically creates a new `0600` archive file next to `base_path` (appending `-1`..`-99` on
/// same-second collisions) with `O_CREAT | O_EXCL | O_NOFOLLOW` so symlinks in world-writable
/// directories such as `/tmp` are never followed or clobbered, and returns the path written.
pub fn save_archive(base_path: &Path, contents: &str) -> Result<PathBuf, String> {
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mode = Mode::RUSR | Mode::WUSR;
    for attempt in 0_u32..100 {
        let candidate = suffixed_path(base_path, attempt);
        match rustix::fs::openat(rustix::fs::CWD, &candidate, flags, mode) {
            Ok(fd) => {
                let mut file = fs::File::from(fd);
                return file
                    .write_all(contents.as_bytes())
                    .and_then(|()| file.flush())
                    .map(|()| candidate.clone())
                    .map_err(|err| locale::words().output_unwritable(&candidate, &err));
            }
            Err(err) if io::Error::from(err).kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => {
                let io_err = io::Error::from(err);
                return Err(locale::words().output_unwritable(&candidate, &io_err));
            }
        }
    }
    let exhausted = io::Error::new(io::ErrorKind::AlreadyExists, "archive path collision");
    Err(locale::words().output_unwritable(base_path, &exhausted))
}

fn suffixed_path(base: &Path, attempt: u32) -> PathBuf {
    if attempt == 0 {
        return base.to_path_buf();
    }
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let stem = base.file_stem().map_or_else(
        || "lowendaff-bench".to_owned(),
        |s| s.to_string_lossy().into_owned(),
    );
    match base.extension().and_then(|e| e.to_str()) {
        Some(ext) if !ext.is_empty() => parent.join(format!("{stem}-{attempt}.{ext}")),
        _ => parent.join(format!("{stem}-{attempt}")),
    }
}

/// Writes `contents` to an explicit user-chosen `path`, returning a localized error on failure.
pub fn save_copy(path: &Path, contents: &str) -> Result<(), String> {
    let mut file = app::create_output(path)?;
    file.write_all(contents.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|err| locale::words().output_unwritable(path, &err))
}

/// Saves an interactive report export to `path`: uses [`save_archive`] (`O_CREAT | O_EXCL |
/// O_NOFOLLOW`, `0600`) when `path` is directly inside `/tmp` so Linux `fs.protected_regular` and
/// same-second collisions never fail or clobber, or [`save_copy`] elsewhere.
pub fn save_user_report(path: &Path, contents: &str) -> Result<PathBuf, String> {
    if path.parent() == Some(Path::new(DEFAULT_REPORT_DIR)) {
        save_archive(path, contents)
    } else {
        save_copy(path, contents).map(|()| path.to_path_buf())
    }
}
