//! Host facts for lowendaff-bench, shared by the orchestrator and the test items.
//!
//! Everything is read from kernel interfaces (`/proc`, `/sys`) and standard files under
//! `/etc`; no program is run (design P-11). Every reader goes through a [`Root`], so tests
//! point it at a captured fixture instead of `/`.

#![warn(missing_docs)]

use std::fs;
use std::path::PathBuf;

pub mod platform;

/// The filesystem host facts are read from: `/` on a real host, a captured copy in tests.
#[derive(Debug, Clone)]
pub struct Root {
    path: PathBuf,
}

impl Root {
    /// The root filesystem of this host.
    #[must_use]
    pub fn system() -> Self {
        Self::new("/")
    }

    /// A directory laid out like `/`, such as a captured fixture.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Where the absolute host path `path` (for example `/proc/cpuinfo`) lives.
    #[must_use]
    pub fn join(&self, path: &str) -> PathBuf {
        self.path.join(path.trim_start_matches('/'))
    }

    /// The file's contents, or `None` when it is missing or unreadable. Invalid UTF-8 is
    /// replaced, because firmware strings such as DMI fields are not always valid.
    #[must_use]
    pub fn read(&self, path: &str) -> Option<String> {
        let bytes = fs::read(self.join(path)).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// The first line of the file without surrounding whitespace; `None` when the file is
    /// missing, unreadable or blank.
    #[must_use]
    pub fn read_line(&self, path: &str) -> Option<String> {
        let text = self.read(path)?;
        let line = text.lines().next()?.trim();
        (!line.is_empty()).then(|| line.to_owned())
    }

    /// Whether the path exists.
    #[must_use]
    pub fn exists(&self, path: &str) -> bool {
        self.join(path).exists()
    }

    /// The names in a directory, sorted; empty when it is missing.
    #[must_use]
    pub fn list(&self, path: &str) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.join(path)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// The target of a symbolic link, such as `pid:[4026531836]` for `/proc/self/ns/pid`.
    #[must_use]
    pub fn link(&self, path: &str) -> Option<String> {
        let target = fs::read_link(self.join(path)).ok()?;
        Some(target.to_string_lossy().into_owned())
    }
}
