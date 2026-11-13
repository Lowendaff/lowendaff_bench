//! The host's C library: its types, and how a program built for glibc would find it. Only files
//! are read; no host program runs.

use std::collections::VecDeque;
use std::fmt;

use crate::Root;

/// A glibc version, such as 2.35. Versions compare by major, then minor number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlibcVersion {
    /// Major number, 2 for every glibc since 1997.
    pub major: u32,
    /// Minor number.
    pub minor: u32,
}

impl fmt::Display for GlibcVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// The host's C library, as far as programs built for glibc are concerned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Libc {
    /// glibc of this version.
    Glibc(GlibcVersion),
    /// glibc whose version could not be read. Plugins that need glibc still run.
    #[default]
    GlibcUnknown,
    /// No glibc loader for this architecture, as on musl systems such as Alpine.
    NoGlibc,
}

/// Symbolic links followed before a path counts as broken, as the kernel's `MAXSYMLINKS`.
const MAX_LINKS: usize = 40;

/// The C library that a glibc program built for `arch` (a value of `std::env::consts::ARCH`)
/// would load. The loader that such programs name decides whether there is glibc at all. The
/// version comes from the loader itself, which describes itself since glibc 2.33 (and in RHEL
/// 8's 2.28), or else from the `libc.so.6` beside it. Neither alone covers every distribution:
/// older loaders do not describe themselves, and RHEL's aarch64 builds keep the loader in
/// `/usr/lib` but `libc.so.6` in `/usr/lib64`.
#[must_use]
pub fn read(root: &Root, arch: &str) -> Libc {
    let loader = match arch {
        "x86_64" => "/lib64/ld-linux-x86-64.so.2",
        "aarch64" => "/lib/ld-linux-aarch64.so.1",
        _ => return Libc::GlibcUnknown,
    };
    let Some(loader) = resolve(root, loader) else {
        return Libc::NoGlibc;
    };
    let beside_the_loader = || {
        let directory = loader
            .rsplit_once('/')
            .map_or("", |(directory, _)| directory);
        resolve(root, &format!("{directory}/libc.so.6"))
            .and_then(|libc| root.read_bytes(&libc))
            .and_then(|bytes| version(&bytes))
    };
    root.read_bytes(&loader)
        .and_then(|bytes| version(&bytes))
        .or_else(beside_the_loader)
        .map_or(Libc::GlibcUnknown, Libc::Glibc)
}

/// The version in glibc's description of itself, such as `stable release version 2.41.`.
fn version(library: &[u8]) -> Option<GlibcVersion> {
    const MARKER: &[u8] = b"release version ";
    let start = library
        .windows(MARKER.len())
        .position(|window| window == MARKER)?
        + MARKER.len();
    let rest = &library[start..];
    let length = rest
        .iter()
        .take_while(|byte| byte.is_ascii_digit() || **byte == b'.')
        .count();
    let text = std::str::from_utf8(&rest[..length]).ok()?;
    let mut numbers = text.split('.');
    let major = numbers.next()?.parse().ok()?;
    let minor = numbers.next()?.parse().ok()?;
    Some(GlibcVersion { major, minor })
}

/// `path` with its symbolic links followed inside `root`, as the kernel follows them from `/`;
/// `None` when it does not exist or the links loop.
fn resolve(root: &Root, path: &str) -> Option<String> {
    let mut pending = components(path);
    let mut resolved: Vec<String> = Vec::new();
    let mut links = 0;
    while let Some(component) = pending.pop_front() {
        match component.as_str() {
            "." => {}
            ".." => {
                resolved.pop();
            }
            _ => {
                resolved.push(component);
                let Some(target) = root.link(&format!("/{}", resolved.join("/"))) else {
                    continue;
                };
                // The link's own name gives way to its target.
                resolved.pop();
                links += 1;
                if links > MAX_LINKS {
                    return None;
                }
                if target.starts_with('/') {
                    resolved.clear();
                }
                let mut parts = components(&target);
                while let Some(part) = parts.pop_back() {
                    pending.push_front(part);
                }
            }
        }
    }
    let path = format!("/{}", resolved.join("/"));
    root.exists(&path).then_some(path)
}

/// The names in a path, without empty ones.
fn components(path: &str) -> VecDeque<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}
