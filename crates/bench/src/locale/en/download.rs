//! English: a download's reasons and `-v` lines.

use std::fmt::Display;
use std::path::Path;

use bench_api::IpFamily;

use super::English;
use crate::locale::Download;
use crate::locale::DownloadWords;
use crate::locale::Retry;

impl DownloadWords for English {
    fn no_build(&self, arch: &str) -> String {
        format!("no build for {arch}")
    }

    fn download_interrupted(&self) -> String {
        "download interrupted".to_owned()
    }

    fn no_ranges(&self, tool: &str, version: &str) -> String {
        format!("{tool} {version}: the server does not serve byte ranges; one connection instead")
    }

    fn in_ranges(&self, tool: &str, version: &str, count: usize) -> String {
        format!("{tool} {version} in {count} byte ranges at once")
    }

    fn retry(&self, retry: &Retry<'_>) -> String {
        let Retry {
            download,
            range,
            attempt,
            wait,
            reason,
        } = retry;
        let download = match download {
            Download::Tool { name, version } => format!("{name} {version}"),
            Download::File { name } => format!("file {name}"),
        };
        match range {
            Some((range, from)) => format!(
                "{download}: byte range {range}, attempt {attempt} failed; going on from byte \
                 {from} in {wait} s: {reason}"
            ),
            None => {
                format!("{download}: attempt {attempt} failed; trying again in {wait} s: {reason}")
            }
        }
    }

    fn request_failed(&self, error: &dyn Display, hint: &str) -> String {
        format!("download failed: {error}{hint}")
    }

    fn http_status(&self, code: u16) -> String {
        format!("download failed: HTTP {code}")
    }

    fn wrong_range(&self) -> String {
        "download failed: the server sent another byte range than the one asked for".to_owned()
    }

    fn ranges_stopped(&self) -> String {
        "download failed: the server stopped serving byte ranges".to_owned()
    }

    fn broke_off(&self, error: &dyn Display) -> String {
        format!("download broke off: {error}")
    }

    fn too_large(&self, size: u64) -> String {
        format!("larger than the pinned {size} bytes")
    }

    fn file_too_large(&self, limit: u64) -> String {
        format!("larger than {limit} bytes")
    }

    fn not_https(&self, url: &str) -> String {
        format!("not an HTTPS address: {url}")
    }

    fn family_unreachable(&self, family: IpFamily) -> String {
        let family = match family {
            IpFamily::Ipv4 => "IPv4",
            IpFamily::Ipv6 => "IPv6",
        };
        format!("not fetched: the host has no {family} connection")
    }

    fn wrong_size(&self, size: u64, actual: u64) -> String {
        format!("size mismatch: {size} bytes pinned, {actual} bytes came")
    }

    fn range_too_large(&self, length: u64) -> String {
        format!("a byte range larger than its {length} bytes")
    }

    fn range_wrong_size(&self, length: u64, actual: u64) -> String {
        format!("size mismatch: a byte range of {length} bytes came with {actual} bytes")
    }

    fn sha_mismatch(&self, pinned: &str, actual: &str) -> String {
        format!("SHA-256 mismatch: {pinned} pinned, {actual} came")
    }

    fn write_failed(&self, error: &dyn Display) -> String {
        format!("cannot write the download: {error}")
    }

    fn read_failed(&self, error: &dyn Display) -> String {
        format!("cannot read the downloaded file: {error}")
    }

    fn install_failed(&self, error: &dyn Display) -> String {
        format!("cannot install: {error}")
    }

    fn raw_sockets_linux_only(&self) -> String {
        "it needs raw sockets, which the program gives on Linux only".to_owned()
    }

    fn raw_sockets_no_new_privs(&self) -> String {
        "it needs raw sockets, but the program runs with no_new_privs, which makes the system \
         ignore file capabilities"
            .to_owned()
    }

    fn raw_sockets_bounded(&self) -> String {
        "it needs raw sockets, but CAP_NET_RAW is not in the program's capability bounding set, \
         as in a container that drops it"
            .to_owned()
    }

    fn raw_sockets_nosuid(&self) -> String {
        "it needs raw sockets, but the run directory's filesystem is mounted nosuid, which \
         ignores file capabilities; choose another with --work-dir"
            .to_owned()
    }

    fn raw_sockets_failed(&self, error: &dyn Display) -> String {
        format!("cannot give it raw sockets (the file capability CAP_NET_RAW): {error}")
    }

    fn unpack_failed(&self, error: &dyn Display) -> String {
        format!("cannot unpack: {error}")
    }

    fn damaged_archive(&self, error: &dyn Display) -> String {
        format!("damaged archive: {error}")
    }

    fn unsupported_entry(&self, path: &Path) -> String {
        format!("the archive holds an unsupported entry: {}", path.display())
    }

    fn no_executable(&self, executable: &str) -> String {
        format!("the archive holds no {executable}")
    }

    fn proxy_set(&self, variable: &str) -> String {
        format!("; {variable} is set on this host: check that the proxy works")
    }

    fn proxy_unset(&self) -> String {
        "; set HTTPS_PROXY or ALL_PROXY to use a proxy".to_owned()
    }
}
