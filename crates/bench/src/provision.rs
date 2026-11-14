//! Tool provisioning: download every pinned tool the plan needs before the
//! first test, verify size and SHA-256, and only then make it executable or unpack it. A large
//! file comes in byte ranges over several connections at once, when its server serves ranges:
//! a slow route often limits each connection rather than the host.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::fs::Permissions;
use std::future::Future;
use std::io;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use bench_api::CancellationToken;
use bench_api::Requirements;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use sha2::Sha256;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncRead;

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
        let installed = match tool.asset_for(arch) {
            None => Err(format!("没有适用于 {arch} 的版本")),
            Some(asset) => {
                tracing::debug!("下载 {}", asset.url);
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
        () = cancel.cancelled() => Err("下载已中断".to_owned()),
    };
    finish(written, &partial, tool, dir).await
}

/// Fetches `asset` into `path` and checks its size and SHA-256: over one connection, or, when
/// the file is large enough and its server serves byte ranges, one range per connection, up to
/// `connections` at once.
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
    let ranges = ranges(asset.size, connections);
    if ranges.len() < 2 {
        let (Part::Whole(body) | Part::Range(body)) = fetch(asset.url, None)
            .await
            .map_err(|err| err.to_string())?;
        return write_verified(body, asset, path)
            .await
            .map_err(|err| err.to_string());
    }
    let first = fetch(asset.url, Some(ranges[0].clone()))
        .await
        .map_err(|err| err.to_string())?;
    match first {
        Part::Whole(body) => {
            tracing::debug!(
                "{} {}：服务器不支持分段下载，改用一个连接",
                tool.name,
                tool.version
            );
            write_verified(body, asset, path)
                .await
                .map_err(|err| err.to_string())
        }
        Part::Range(first) => {
            tracing::debug!(
                "{} {} 分 {} 段同时下载",
                tool.name,
                tool.version,
                ranges.len()
            );
            write_ranges(fetch, first, &ranges, asset, path).await
        }
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
    outcome
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

    let write_error = |err: io::Error| DownloadError::Final(format!("无法写入：{err}"));
    let mut file = File::create(path).await.map_err(write_error)?;
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = body
            .read(&mut buffer)
            .await
            .map_err(|err| DownloadError::Transient(format!("下载中断：{err}")))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > asset.size {
            return Err(DownloadError::Final(format!(
                "大小超过声明的 {} 字节",
                asset.size
            )));
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).await.map_err(write_error)?;
    }
    file.flush().await.map_err(write_error)?;
    if total != asset.size {
        return Err(DownloadError::Transient(format!(
            "大小不符：应为 {} 字节，实际 {total} 字节",
            asset.size
        )));
    }
    pinned(hasher, asset).map_err(DownloadError::Final)
}

/// Writes the byte `ranges` of `asset` into `path`, all at once: the first from `first`, which
/// its server has begun to send, every other over a connection of its own. Then checks the whole
/// file's SHA-256.
async fn write_ranges<F, Fut, R>(
    fetch: &F,
    first: R,
    ranges: &[Range<u64>],
    asset: &ToolAsset,
    path: &Path,
) -> Result<(), String>
where
    F: Fn(&'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    File::create(path)
        .await
        .map_err(|err| format!("无法写入：{err}"))?;
    let others = ranges[1..].iter().map(|range| async move {
        match fetch(asset.url, Some(range.clone())).await? {
            Part::Range(body) => write_range(body, range, path).await,
            Part::Whole(_) => Err(DownloadError::Final(
                "下载失败：服务器不再分段返回".to_owned(),
            )),
        }
    });
    futures_util::future::try_join(
        write_range(first, &ranges[0], path),
        futures_util::future::try_join_all(others),
    )
    .await
    .map_err(|err| err.to_string())?;
    check(asset, path).await
}

/// Writes `body`, which holds the byte `range` of a file, at its place in `path`; it must hold
/// exactly that many bytes. A body that breaks off or ends short may come whole on another try;
/// one that runs long would not.
async fn write_range<R: AsyncRead + Unpin>(
    mut body: R,
    range: &Range<u64>,
    path: &Path,
) -> Result<(), DownloadError> {
    use std::io::SeekFrom;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncSeekExt;
    use tokio::io::AsyncWriteExt;

    let write_error = |err: io::Error| DownloadError::Final(format!("无法写入：{err}"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .await
        .map_err(write_error)?;
    file.seek(SeekFrom::Start(range.start))
        .await
        .map_err(write_error)?;
    let length = range.end - range.start;
    let mut total: u64 = 0;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = body
            .read(&mut buffer)
            .await
            .map_err(|err| DownloadError::Transient(format!("下载中断：{err}")))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > length {
            return Err(DownloadError::Final(format!(
                "大小超过一段应有的 {length} 字节"
            )));
        }
        file.write_all(&buffer[..read]).await.map_err(write_error)?;
    }
    file.flush().await.map_err(write_error)?;
    if total != length {
        return Err(DownloadError::Transient(format!(
            "大小不符：一段应为 {length} 字节，实际 {total} 字节"
        )));
    }
    Ok(())
}

/// Checks the SHA-256 of the whole file at `path` against `asset`'s pin.
async fn check(asset: &ToolAsset, path: &Path) -> Result<(), String> {
    use sha2::Digest;
    use tokio::io::AsyncReadExt;

    let read_error = |err: io::Error| format!("无法读取下载的文件：{err}");
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
        return Err(format!(
            "SHA-256 不符：应为 {}，实际 {digest}",
            asset.sha256
        ));
    }
    Ok(())
}

async fn make_executable(partial: &Path, installed: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let ready = async {
        fs::set_permissions(partial, Permissions::from_mode(0o755)).await?;
        fs::rename(partial, installed).await
    };
    ready.await.map_err(|err| format!("安装失败：{err}"))
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
            .unwrap_or_else(|err| Err(format!("解压失败：{err}")))
    };
    let outcome = match unpacked {
        Ok(()) => match fs::metadata(staging.join(executable)).await {
            Ok(metadata) if metadata.is_file() => fs::rename(&staging, &installed)
                .await
                .map_err(|err| format!("安装失败：{err}")),
            _ => Err(format!("压缩包里没有 {executable}")),
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

    let damaged = |err: io::Error| format!("压缩包损坏：{err}");
    let write_error = |err: io::Error| format!("解压失败：{err}");
    let file = std::fs::File::open(archive).map_err(write_error)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    std::fs::create_dir(staging).map_err(write_error)?;
    let entries = archive.entries().map_err(damaged)?;
    let mut top: Option<OsString> = None;
    for entry in entries {
        let mut entry = entry.map_err(damaged)?;
        let path = entry.path().map_err(damaged)?.into_owned();
        let unsupported = || format!("压缩包里有不支持的条目：{}", path.display());
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
