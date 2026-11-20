//! The reasons of a download that failed, of a tool or of a file that a plugin asked for, and the
//! lines that `-v` shows of a download.

use std::fmt::Display;
use std::path::Path;

use bench_api::IpFamily;

/// What a download gets, as the `-v` log names it.
#[derive(Debug, Clone, Copy)]
pub enum Download<'a> {
    /// A tool's build.
    Tool {
        /// The tool's name.
        name: &'a str,
        /// The tool's version.
        version: &'a str,
    },
    /// A file that a plugin asked for.
    File {
        /// The file's name.
        name: &'a str,
    },
}

/// A failed attempt at a download, which is tried again: for the `-v` log.
pub struct Retry<'a> {
    /// What the download gets.
    pub download: Download<'a>,
    /// For a byte range, its number, from 1, and the byte that the next attempt goes on from;
    /// `None` for a whole file, which starts over.
    pub range: Option<(usize, u64)>,
    /// The number of the attempt that failed, from 1.
    pub attempt: u32,
    /// The seconds until the next attempt.
    pub wait: u64,
    /// Why the attempt failed.
    pub reason: &'a str,
}

/// A download's reasons and `-v` lines.
pub trait DownloadWords {
    /// No build of a tool for the host's architecture.
    fn no_build(&self, arch: &str) -> String;
    /// A download that Ctrl-C stopped.
    fn download_interrupted(&self) -> String;
    /// `-v`: a server that answered a byte range with the whole file, which comes over one
    /// connection.
    fn no_ranges(&self, tool: &str, version: &str) -> String;
    /// `-v`: a file that comes in byte ranges, one per connection, all at once.
    fn in_ranges(&self, tool: &str, version: &str, count: usize) -> String;
    /// `-v`: a failed attempt, which is tried again.
    fn retry(&self, retry: &Retry<'_>) -> String;
    /// A request that got no answer, with the proxy hint at its end.
    fn request_failed(&self, error: &dyn Display, hint: &str) -> String;
    /// An answer with another HTTP status than success.
    fn http_status(&self, code: u16) -> String;
    /// An answer with another byte range than the one asked for.
    fn wrong_range(&self) -> String;
    /// A server that answered a later byte range with the whole file.
    fn ranges_stopped(&self) -> String;
    /// A body that broke off or stalled.
    fn broke_off(&self, error: &dyn Display) -> String;
    /// A whole file larger than its pinned size.
    fn too_large(&self, size: u64) -> String;
    /// A file that a plugin asked for, larger than the files that a plugin may ask for.
    fn file_too_large(&self, limit: u64) -> String;
    /// A file that a plugin asked for, at an address that is not HTTPS.
    fn not_https(&self, url: &str) -> String;
    /// A file to fetch over an IP version that the host cannot reach, which is not fetched.
    fn family_unreachable(&self, family: IpFamily) -> String;
    /// A whole file of another size than its pinned one.
    fn wrong_size(&self, size: u64, actual: u64) -> String;
    /// A byte range larger than its length.
    fn range_too_large(&self, length: u64) -> String;
    /// A byte range of another size than its length.
    fn range_wrong_size(&self, length: u64, actual: u64) -> String;
    /// A file whose SHA-256 is not its pinned one.
    fn sha_mismatch(&self, pinned: &str, actual: &str) -> String;
    /// A partial file that could not be written.
    fn write_failed(&self, error: &dyn Display) -> String;
    /// A downloaded file that could not be read back for its check.
    fn read_failed(&self, error: &dyn Display) -> String;
    /// A checked tool that could not be made executable or moved in place.
    fn install_failed(&self, error: &dyn Display) -> String;
    /// A tool that needs raw sockets, on a system other than Linux.
    fn raw_sockets_linux_only(&self) -> String;
    /// A tool that needs raw sockets, in a process with `no_new_privs`.
    fn raw_sockets_no_new_privs(&self) -> String;
    /// A tool that needs raw sockets, in a process whose bounding set lacks CAP_NET_RAW.
    fn raw_sockets_bounded(&self) -> String;
    /// A tool that needs raw sockets, in a run directory on a filesystem mounted `nosuid`.
    fn raw_sockets_nosuid(&self) -> String;
    /// A tool whose file capability could not be set.
    fn raw_sockets_failed(&self, error: &dyn Display) -> String;
    /// An archive that could not be unpacked.
    fn unpack_failed(&self, error: &dyn Display) -> String;
    /// An archive that holds a damaged entry.
    fn damaged_archive(&self, error: &dyn Display) -> String;
    /// An archive entry that is no regular file or directory, or that could land outside.
    fn unsupported_entry(&self, path: &Path) -> String;
    /// An archive without the tool's executable.
    fn no_executable(&self, executable: &str) -> String;
    /// The end of a network error's reason when a download proxy option is set: the option name,
    /// never its value, which may hold credentials.
    fn proxy_set(&self, option: &str) -> String;
    /// The end of a network error's reason when no download proxy is set: how to pass one.
    fn proxy_unset(&self) -> String;
}
