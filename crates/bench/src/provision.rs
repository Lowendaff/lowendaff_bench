//! Tool provisioning: download every pinned tool the plan needs before the
//! first test, verify size and SHA-256, and only then make it executable or unpack it, and give
//! a tool that sends raw packets CAP_NET_RAW; and the files that the plugins ask for, which are
//! not pinned, only limited in size. A large
//! file comes in byte ranges over several connections at once, when its server serves ranges:
//! a slow route often limits each connection rather than the host. A download that fails in a
//! way that may pass is tried again after a wait: a byte range from its first missing byte, a
//! whole file from the start.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::fs::Permissions;
use std::future::Future;
use std::io;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::FileSource;
use bench_api::FileSpec;
use bench_api::IpFamily;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use sha2::Sha256;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncRead;

use crate::locale;
use crate::locale::Download;
use crate::locale::Retry;
use crate::progress;

/// Verified tools by name, plus the user-facing reason for every tool that failed
/// (the progress log shows it next to the tool's name and version, and a plugin that needs
/// the tool is skipped with it).
#[derive(Debug, Default)]
pub struct Provisioned {
    /// Paths of verified, executable tools.
    pub ready: BTreeMap<String, PathBuf>,
    /// Why each remaining tool is unavailable.
    pub failed: BTreeMap<String, String>,
}

/// The smallest byte range that gets a connection of its own: a file smaller than two of them
/// comes over one connection.
pub const MIN_RANGE: u64 = 1 << 20;

/// The attempts that one download, or one byte range of it, gets.
const ATTEMPTS: u32 = 4;

/// The largest file that a plugin may ask for: such a file is a list, not a tool.
pub const FILE_LIMIT: u64 = 64 * 1024;

/// The wait before the first retry; it doubles before each further one.
const FIRST_WAIT: Duration = Duration::from_secs(1);

/// Why a download failed, in the user's words, and whether another try may go better.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    /// It may pass on its own: no answer came, the server was busy or failing for the moment, or
    /// the body broke off, stalled or ended short.
    Transient(String),
    /// Another try would end the same way.
    Final(String),
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (Self::Transient(reason) | Self::Final(reason)) = self;
        f.write_str(reason)
    }
}

/// What a download returned.
pub enum Part<R> {
    /// The whole file: asked for so, or the server ignored the byte range that was asked for.
    Whole(R),
    /// Exactly the byte range that was asked for.
    Range(R),
}

/// The failed attempts at one download, or at one byte range of it, since the count began.
#[derive(Default)]
struct Attempts {
    failed: u32,
}

impl Attempts {
    /// After a failed attempt at `download`: logs the retry and waits, 1, 2 and then 4 seconds,
    /// or gives the reason when the failure would end the same way again or the attempts are
    /// used up. `range` is the byte range's number, from 1, and the byte that the next attempt
    /// goes on from; `None` for a whole file.
    async fn again(
        &mut self,
        error: DownloadError,
        download: Download<'_>,
        range: Option<(usize, u64)>,
    ) -> Result<(), String> {
        let reason = match error {
            DownloadError::Final(reason) => return Err(reason),
            DownloadError::Transient(reason) => reason,
        };
        self.failed += 1;
        if self.failed >= ATTEMPTS {
            return Err(reason);
        }
        let wait = FIRST_WAIT * 2_u32.pow(self.failed - 1);
        let retry = Retry {
            download,
            range,
            attempt: self.failed,
            wait: wait.as_secs(),
            reason: &reason,
        };
        tracing::debug!("{}", locale::words().retry(&retry));
        tokio::time::sleep(wait).await;
        Ok(())
    }
}

/// A tool's download, as the log names it.
fn of(tool: &ToolSpec) -> Download<'_> {
    Download::Tool {
        name: tool.name,
        version: tool.version,
    }
}

/// Unique tools needed by the planned plugins, in plan order.
pub fn required_tools(requirements: impl IntoIterator<Item = Requirements>) -> Vec<ToolSpec> {
    let mut tools: Vec<ToolSpec> = Vec::new();
    for requires in requirements {
        for tool in requires.tools {
            if !tools.iter().any(|known| known.name == tool.name) {
                tools.push(*tool);
            }
        }
    }
    tools
}

/// Installs every tool into `dir`, and logs how each one went. `fetch` opens the download for a
/// URL, the whole file or a byte range of it (HTTPS in production, an in-memory body in tests),
/// and says why it failed. A file comes over up to `connections` connections at once, one byte
/// range each. When `cancel` fires, the download under way stops at once and fails, and the
/// tools after it are not downloaded.
pub async fn provision_all<F, Fut, R>(
    tools: &[ToolSpec],
    arch: &str,
    dir: &Path,
    connections: usize,
    cancel: &CancellationToken,
    fetch: F,
) -> Provisioned
where
    F: Fn(&'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    let mut result = Provisioned::default();
    for tool in tools {
        if cancel.is_cancelled() {
            break;
        }
        // A tool that needs raw sockets is not downloaded where it could not have them.
        let possible = if tool.raw_sockets {
            raw_sockets_possible(dir)
        } else {
            Ok(())
        };
        let installed = match (tool.asset_for(arch), possible) {
            (None, _) => Err(locale::words().no_build(arch)),
            (Some(_), Err(reason)) => Err(reason),
            (Some(asset), Ok(())) => {
                tracing::debug!("{}", locale::words().download(asset.url));
                download(&fetch, tool, asset, dir, connections, cancel).await
            }
        };
        match installed {
            Ok(path) => {
                result.ready.insert(tool.name.to_owned(), path);
            }
            Err(reason) => {
                result.failed.insert(tool.name.to_owned(), reason);
            }
        }
    }
    progress::tools(tools, &result);
    result
}

/// Gets the files that a plugin asked for: a file on the host is handed on as it is, and one at
/// an HTTPS address is downloaded into `dir`, whole, at most [`FILE_LIMIT`] bytes, and tried again
/// after a failure that may pass, as a tool is, over the file's family alone when it names one.
/// A file of a family that is not among `families`, those that the host reaches, is not fetched.
/// When `cancel` fires, the download under way stops at once and fails, and the files after it
/// are not downloaded.
pub async fn fetch_files<F, Fut, R>(
    files: &[FileSpec],
    dir: &Path,
    families: &[IpFamily],
    cancel: &CancellationToken,
    fetch: &F,
) -> Provisioned
where
    F: Fn(Option<IpFamily>, &'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    let mut result = Provisioned::default();
    for file in files {
        if cancel.is_cancelled() {
            break;
        }
        let had = match (&file.source, file.family) {
            (FileSource::Path(path), _) => Ok(path.clone()),
            (FileSource::Url(_), Some(family)) if !families.contains(&family) => {
                Err(locale::words().family_unreachable(family))
            }
            (FileSource::Url(url), _) if !url.starts_with("https://") => {
                Err(locale::words().not_https(url))
            }
            (FileSource::Url(url), _) => {
                tracing::debug!("{}", locale::words().download(url));
                // The fetch takes an address that lasts as long as the program, as a tool's
                // pinned one does; a file's comes from the configuration, so it is kept to the end.
                let url: &'static str = Box::leak(url.clone().into_boxed_str());
                let path = dir.join(file.name);
                let over = |url, range| fetch(file.family, url, range);
                let fetched = tokio::select! {
                    fetched = fetch_file(&over, file.name, url, &path, dir) => fetched,
                    () = cancel.cancelled() => Err(locale::words().download_interrupted()),
                };
                if fetched.is_err() {
                    let _ = fs::remove_file(&path).await;
                }
                fetched.map(|()| path)
            }
        };
        match had {
            Ok(path) => {
                result.ready.insert(file.name.to_owned(), path);
            }
            Err(reason) => {
                result.failed.insert(file.name.to_owned(), reason);
            }
        }
    }
    result
}

/// Downloads the file `name` from `url` into `path`, in the directory `dir`, which it makes first,
/// and starts over after a failure that may pass.
async fn fetch_file<F, Fut, R>(
    fetch: &F,
    name: &str,
    url: &'static str,
    path: &Path,
    dir: &Path,
) -> Result<(), String>
where
    F: Fn(&'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    fs::create_dir_all(dir)
        .await
        .map_err(|err| locale::words().write_failed(&err))?;
    let mut attempts = Attempts::default();
    loop {
        let written = match fetch(url, None).await {
            Ok(Part::Whole(body) | Part::Range(body)) => write_limited(body, path).await,
            Err(error) => Err(error),
        };
        let Err(error) = written else {
            return Ok(());
        };
        attempts.again(error, Download::File { name }, None).await?;
    }
}

/// Streams `body`, a file that a plugin asked for, into `path`, at most [`FILE_LIMIT`] bytes. A
/// body that breaks off may come whole on another try; one that grows past the limit would not.
async fn write_limited<R: AsyncRead + Unpin>(
    mut body: R,
    path: &Path,
) -> Result<(), DownloadError> {
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    let write_error = |err: io::Error| DownloadError::Final(locale::words().write_failed(&err));
    let mut file = File::create(path).await.map_err(write_error)?;
    let mut total: u64 = 0;
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        let read = body
            .read(&mut buffer)
            .await
            .map_err(|err| DownloadError::Transient(locale::words().broke_off(&err)))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > FILE_LIMIT {
            return Err(DownloadError::Final(
                locale::words().file_too_large(FILE_LIMIT),
            ));
        }
        file.write_all(&buffer[..read]).await.map_err(write_error)?;
    }
    file.flush().await.map_err(write_error)
}

/// Downloads `asset` and installs it, unless `cancel` fires first: then every connection of
/// the download closes at once, and the partial file is removed. Unpacking is not stopped: it
/// is short, and it runs on a thread that would go on regardless.
async fn download<F, Fut, R>(
    fetch: &F,
    tool: &ToolSpec,
    asset: &ToolAsset,
    dir: &Path,
    connections: usize,
    cancel: &CancellationToken,
) -> Result<PathBuf, String>
where
    F: Fn(&'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    let partial = partial_path(dir, tool);
    let written = tokio::select! {
        written = fetch_checked(fetch, tool, asset, &partial, connections) => written,
        () = cancel.cancelled() => Err(locale::words().download_interrupted()),
    };
    finish(written, &partial, tool, dir).await
}

/// Fetches `asset` into `path` and checks its size and SHA-256: a file under two byte ranges
/// whole, over one connection; a larger one, when its server serves byte ranges, one range per
/// connection, up to `connections` at once. With one connection, a larger file is one range, so
/// that it can continue after a break. Failures before the first answer count against the first
/// range.
async fn fetch_checked<F, Fut, R>(
    fetch: &F,
    tool: &ToolSpec,
    asset: &ToolAsset,
    path: &Path,
    connections: usize,
) -> Result<(), String>
where
    F: Fn(&'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    if asset.size < 2 * MIN_RANGE {
        return fetch_whole(fetch, tool, asset, path, None).await;
    }
    let ranges = ranges(asset.size, connections);
    let mut attempts = Attempts::default();
    let first = loop {
        match fetch(asset.url, Some(ranges[0].clone())).await {
            Ok(part) => break part,
            Err(error) => attempts.again(error, of(tool), None).await?,
        }
    };
    match first {
        Part::Whole(body) => {
            tracing::debug!("{}", locale::words().no_ranges(tool.name, tool.version));
            fetch_whole(fetch, tool, asset, path, Some((body, attempts))).await
        }
        Part::Range(body) => {
            if ranges.len() > 1 {
                let line = locale::words().in_ranges(tool.name, tool.version, ranges.len());
                tracing::debug!("{line}");
            }
            write_ranges(fetch, tool, asset, path, &ranges, (body, attempts)).await
        }
    }
}

/// Fetches the whole of `asset` into `path` over one connection, beginning with `first`, an
/// answer that has come already and the attempts that it took. After a failure that may pass,
/// it starts over.
async fn fetch_whole<F, Fut, R>(
    fetch: &F,
    tool: &ToolSpec,
    asset: &ToolAsset,
    path: &Path,
    first: Option<(R, Attempts)>,
) -> Result<(), String>
where
    F: Fn(&'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    let (mut body, mut attempts) = match first {
        Some((body, attempts)) => (Some(body), attempts),
        None => (None, Attempts::default()),
    };
    loop {
        let written = match body.take() {
            Some(body) => write_verified(body, asset, path).await,
            None => match fetch(asset.url, None).await {
                Ok(Part::Whole(body) | Part::Range(body)) => {
                    write_verified(body, asset, path).await
                }
                Err(error) => Err(error),
            },
        };
        let Err(error) = written else {
            return Ok(());
        };
        attempts.again(error, of(tool), None).await?;
    }
}

/// Splits `size` bytes into consecutive ranges, one per connection: at most `connections` of
/// them, and none smaller than [`MIN_RANGE`] unless it is the only one. When the size does not
/// divide evenly, the first ranges are a byte longer.
fn ranges(size: u64, connections: usize) -> Vec<Range<u64>> {
    let count = (size / MIN_RANGE).clamp(1, connections.max(1) as u64);
    let (base, extra) = (size / count, size % count);
    let mut start = 0;
    (0..count)
        .map(|index| {
            let length = base + u64::from(index < extra);
            let range = start..start + length;
            start += length;
            range
        })
        .collect()
}

/// Streams `body` into `dir` and installs it as `tool.form` says: an executable becomes
/// `dir/<name>`, and an archive is unpacked into the directory `dir/<name>`. The size limit
/// is enforced while reading, the SHA-256 is checked before anything
/// becomes executable or is unpacked, and on any failure the partial files are removed.
/// Returns the path of the executable.
pub async fn install<R: AsyncRead + Unpin>(
    body: R,
    tool: &ToolSpec,
    asset: &ToolAsset,
    dir: &Path,
) -> Result<PathBuf, String> {
    let partial = partial_path(dir, tool);
    let written = write_verified(body, asset, &partial)
        .await
        .map_err(|err| err.to_string());
    finish(written, &partial, tool, dir).await
}

/// Where a tool's download stays until it has been checked.
fn partial_path(dir: &Path, tool: &ToolSpec) -> PathBuf {
    dir.join(format!("{}.part", tool.name))
}

/// Installs the checked download at `partial` as `tool.form` says, unless writing it failed,
/// and removes the partial file in either case.
async fn finish(
    written: Result<(), String>,
    partial: &Path,
    tool: &ToolSpec,
    dir: &Path,
) -> Result<PathBuf, String> {
    let installed = dir.join(tool.name);
    let outcome = match written {
        Ok(()) => match tool.form {
            ToolForm::Executable => make_executable(partial, &installed)
                .await
                .map(|()| installed),
            ToolForm::TarGz { executable } => unpack(partial, dir, tool.name, executable).await,
        },
        Err(err) => Err(err),
    };
    // Left only after a failure, or as the archive that is unpacked now.
    let _ = fs::remove_file(partial).await;
    match outcome {
        Ok(path) if tool.raw_sockets => grant_raw_sockets(&path).map(|()| path),
        outcome => outcome,
    }
}

/// The file capability that gives a tool raw sockets, `cap_net_raw+ep`, as `setcap` writes it in
/// the attribute `security.capability`: a `vfs_cap_data` of revision 2 (`0x02000000`) with the
/// effective flag (`0x1`), and CAP_NET_RAW (bit 13) the one permitted capability, in
/// little-endian words: magic, then the permitted and inheritable sets of capabilities 0 to 31,
/// then of 32 to 63.
pub const RAW_SOCKETS: [u8; 20] = [
    0x01, 0x00, 0x00, 0x02, // the revision and the effective flag
    0x00, 0x20, 0x00, 0x00, // permitted: CAP_NET_RAW
    0x00, 0x00, 0x00, 0x00, // inheritable: none
    0x00, 0x00, 0x00, 0x00, // permitted, 32 to 63: none
    0x00, 0x00, 0x00, 0x00, // inheritable, 32 to 63: none
];

/// Whether a tool in `dir` can have raw sockets from a file capability: the system ignores file
/// capabilities in a process with `no_new_privs`, and on a filesystem mounted `nosuid`; and it
/// refuses to start a program whose capability it cannot give, as when CAP_NET_RAW is not in the
/// bounding set, which a container may drop it from. A state that cannot be read is no reason:
/// the capability's own failure then says what went wrong.
#[cfg(target_os = "linux")]
fn raw_sockets_possible(dir: &Path) -> Result<(), String> {
    use rustix::fs::StatVfsMountFlags;
    use rustix::thread::CapabilitySet;

    if rustix::thread::no_new_privs().unwrap_or(false) {
        return Err(locale::words().raw_sockets_no_new_privs());
    }
    if !rustix::thread::capability_is_in_bounding_set(CapabilitySet::NET_RAW).unwrap_or(true) {
        return Err(locale::words().raw_sockets_bounded());
    }
    match rustix::fs::statvfs(dir) {
        Ok(stat) if stat.f_flag.contains(StatVfsMountFlags::NOSUID) => {
            Err(locale::words().raw_sockets_nosuid())
        }
        _ => Ok(()),
    }
}

/// File capabilities are Linux's.
#[cfg(not(target_os = "linux"))]
fn raw_sockets_possible(_dir: &Path) -> Result<(), String> {
    Err(locale::words().raw_sockets_linux_only())
}

/// Gives the tool at `path` raw sockets ([`RAW_SOCKETS`]); it needs root, or CAP_SETFCAP.
#[cfg(target_os = "linux")]
fn grant_raw_sockets(path: &Path) -> Result<(), String> {
    rustix::fs::setxattr(
        path,
        "security.capability",
        &RAW_SOCKETS,
        rustix::fs::XattrFlags::empty(),
    )
    .map_err(|err| locale::words().raw_sockets_failed(&io::Error::from(err)))
}

/// File capabilities are Linux's.
#[cfg(not(target_os = "linux"))]
fn grant_raw_sockets(_path: &Path) -> Result<(), String> {
    Err(locale::words().raw_sockets_linux_only())
}

/// Streams `body`, the whole file, into `path`, and checks its size and SHA-256. A body that
/// breaks off or ends short may come whole on another try; one that grows past the pinned size,
/// or holds other bytes, would not.
async fn write_verified<R: AsyncRead + Unpin>(
    mut body: R,
    asset: &ToolAsset,
    path: &Path,
) -> Result<(), DownloadError> {
    use sha2::Digest;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    let write_error = |err: io::Error| DownloadError::Final(locale::words().write_failed(&err));
    let mut file = File::create(path).await.map_err(write_error)?;
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = body
            .read(&mut buffer)
            .await
            .map_err(|err| DownloadError::Transient(locale::words().broke_off(&err)))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > asset.size {
            return Err(DownloadError::Final(locale::words().too_large(asset.size)));
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).await.map_err(write_error)?;
    }
    file.flush().await.map_err(write_error)?;
    if total != asset.size {
        return Err(DownloadError::Transient(
            locale::words().wrong_size(asset.size, total),
        ));
    }
    pinned(hasher, asset).map_err(DownloadError::Final)
}

/// Writes the byte `ranges` of `asset` into `path`, all at once, each over a connection of its
/// own, the first beginning with `first`: its server's answer and the attempts that it took. Then
/// checks the whole file's SHA-256.
async fn write_ranges<F, Fut, R>(
    fetch: &F,
    tool: &ToolSpec,
    asset: &ToolAsset,
    path: &Path,
    ranges: &[Range<u64>],
    first: (R, Attempts),
) -> Result<(), String>
where
    F: Fn(&'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    File::create(path)
        .await
        .map_err(|err| locale::words().write_failed(&err))?;
    let others = ranges
        .iter()
        .enumerate()
        .skip(1)
        .map(|(index, range)| fetch_range(fetch, tool, asset, path, index, range, None));
    futures_util::future::try_join(
        fetch_range(fetch, tool, asset, path, 0, &ranges[0], Some(first)),
        futures_util::future::try_join_all(others),
    )
    .await?;
    check(asset, path).await
}

/// Fetches the byte `range` of `asset`, numbered `index` from 0, into its place in `path`,
/// beginning with `first`, an answer that has come already and the attempts that it took. After
/// a failure that may pass, it asks again from the first missing byte; an attempt that brought
/// [`MIN_RANGE`] bytes or more starts the count afresh, so that a long download that breaks now
/// and then goes on.
async fn fetch_range<F, Fut, R>(
    fetch: &F,
    tool: &ToolSpec,
    asset: &ToolAsset,
    path: &Path,
    index: usize,
    range: &Range<u64>,
    first: Option<(R, Attempts)>,
) -> Result<(), String>
where
    F: Fn(&'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    let (mut body, mut attempts) = match first {
        Some((body, attempts)) => (Some(body), attempts),
        None => (None, Attempts::default()),
    };
    let mut written = 0;
    loop {
        let before = written;
        let result = match body.take() {
            Some(body) => write_range(body, range, &mut written, path).await,
            None => match fetch(asset.url, Some(range.start + written..range.end)).await {
                Ok(Part::Range(body)) => write_range(body, range, &mut written, path).await,
                Ok(Part::Whole(_)) => Err(DownloadError::Final(locale::words().ranges_stopped())),
                Err(error) => Err(error),
            },
        };
        let Err(error) = result else {
            return Ok(());
        };
        if written - before >= MIN_RANGE {
            attempts = Attempts::default();
        }
        let resume = range.start + written;
        attempts
            .again(error, of(tool), Some((index + 1, resume)))
            .await?;
    }
}

/// Writes `body` into the byte `range` of `path`, after the `written` bytes of it that are there
/// already, and adds to `written` what it writes; the range must come to exactly its length. A
/// body that breaks off or ends short may come whole on another try; one that runs long would
/// not.
async fn write_range<R: AsyncRead + Unpin>(
    mut body: R,
    range: &Range<u64>,
    written: &mut u64,
    path: &Path,
) -> Result<(), DownloadError> {
    use std::io::SeekFrom;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncSeekExt;
    use tokio::io::AsyncWriteExt;

    let write_error = |err: io::Error| DownloadError::Final(locale::words().write_failed(&err));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .await
        .map_err(write_error)?;
    file.seek(SeekFrom::Start(range.start + *written))
        .await
        .map_err(write_error)?;
    let length = range.end - range.start;
    let mut buffer = vec![0_u8; 64 * 1024];
    let ended = loop {
        let read = match body.read(&mut buffer).await {
            Ok(0) => break Ok(()),
            Ok(read) => read,
            Err(err) => break Err(DownloadError::Transient(locale::words().broke_off(&err))),
        };
        if *written + read as u64 > length {
            break Err(DownloadError::Final(
                locale::words().range_too_large(length),
            ));
        }
        file.write_all(&buffer[..read]).await.map_err(write_error)?;
        *written += read as u64;
    };
    // What came before a break stays, and the next try goes on after it.
    file.flush().await.map_err(write_error)?;
    ended?;
    if *written != length {
        return Err(DownloadError::Transient(
            locale::words().range_wrong_size(length, *written),
        ));
    }
    Ok(())
}

/// Checks the SHA-256 of the whole file at `path` against `asset`'s pin.
async fn check(asset: &ToolAsset, path: &Path) -> Result<(), String> {
    use sha2::Digest;
    use tokio::io::AsyncReadExt;

    let read_error = |err: io::Error| locale::words().read_failed(&err);
    let mut file = File::open(path).await.map_err(read_error)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await.map_err(read_error)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    pinned(hasher, asset)
}

/// Whether the digest in `hasher` is the SHA-256 that `asset` pins.
fn pinned(hasher: Sha256, asset: &ToolAsset) -> Result<(), String> {
    use sha2::Digest;
    use std::fmt::Write;

    let mut digest = String::with_capacity(64);
    for byte in hasher.finalize() {
        let _ = write!(digest, "{byte:02x}");
    }
    if digest != asset.sha256 {
        return Err(locale::words().sha_mismatch(asset.sha256, &digest));
    }
    Ok(())
}

async fn make_executable(partial: &Path, installed: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let ready = async {
        fs::set_permissions(partial, Permissions::from_mode(0o755)).await?;
        fs::rename(partial, installed).await
    };
    ready
        .await
        .map_err(|err| locale::words().install_failed(&err))
}

/// Unpacks the verified archive into the directory `dir/<name>` and returns the path of
/// `executable` in it. The files go to a staging directory first, which is removed again on
/// any failure, so that the tool's directory appears only complete.
async fn unpack(
    archive: &Path,
    dir: &Path,
    name: &str,
    executable: &str,
) -> Result<PathBuf, String> {
    let staging = dir.join(format!("{name}.unpack"));
    let installed = dir.join(name);
    let unpacked = {
        let (archive, staging) = (archive.to_path_buf(), staging.clone());
        tokio::task::spawn_blocking(move || unpack_into(&archive, &staging))
            .await
            .unwrap_or_else(|err| Err(locale::words().unpack_failed(&err)))
    };
    let outcome = match unpacked {
        Ok(()) => match fs::metadata(staging.join(executable)).await {
            Ok(metadata) if metadata.is_file() => fs::rename(&staging, &installed)
                .await
                .map_err(|err| locale::words().install_failed(&err)),
            _ => Err(locale::words().no_executable(executable)),
        },
        Err(err) => Err(err),
    };
    if outcome.is_err() {
        let _ = fs::remove_dir_all(&staging).await;
    }
    outcome.map(|()| installed.join(executable))
}

/// Writes the entries of a gzip-compressed tar archive under `staging`, without their one
/// top-level directory. Only regular files and directories are accepted, and only paths of
/// plain names, so that nothing lands outside `staging`.
fn unpack_into(archive: &Path, staging: &Path) -> Result<(), String> {
    use std::path::Component;

    let damaged = |err: io::Error| locale::words().damaged_archive(&err);
    let write_error = |err: io::Error| locale::words().unpack_failed(&err);
    let file = std::fs::File::open(archive).map_err(write_error)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    std::fs::create_dir(staging).map_err(write_error)?;
    let entries = archive.entries().map_err(damaged)?;
    let mut top: Option<OsString> = None;
    for entry in entries {
        let mut entry = entry.map_err(damaged)?;
        let path = entry.path().map_err(damaged)?.into_owned();
        let unsupported = || locale::words().unsupported_entry(&path);
        let mut components = path.components();
        let Some(Component::Normal(first)) = components.next() else {
            return Err(unsupported());
        };
        match &top {
            None => top = Some(first.to_owned()),
            Some(top) if top.as_os_str() != first => return Err(unsupported()),
            Some(_) => {}
        }
        let mut target = staging.to_path_buf();
        for component in components {
            let Component::Normal(name) = component else {
                return Err(unsupported());
            };
            target.push(name);
        }
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            std::fs::create_dir_all(&target).map_err(write_error)?;
        } else if kind.is_file() && target != staging {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(write_error)?;
            }
            entry.unpack(&target).map_err(write_error)?;
        } else {
            return Err(unsupported());
        }
    }
    Ok(())
}
