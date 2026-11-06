//! Tool provisioning (design D-13): download every pinned tool the plan needs before the
//! first test, verify size and SHA-256, and only then make it executable.

use std::collections::BTreeMap;
use std::fs::Permissions;
use std::future::Future;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use bench_api::Descriptor;
use bench_api::ToolAsset;
use bench_api::ToolSpec;
use sha2::Sha256;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncRead;

/// Verified tools by name, plus the user-facing reason for every tool that failed
/// (the renderer prints it next to the tool's name and version).
#[derive(Debug, Default)]
pub struct Provisioned {
    /// Paths of verified, executable tools.
    pub ready: BTreeMap<String, PathBuf>,
    /// Why each remaining tool is unavailable.
    pub failed: BTreeMap<String, String>,
}

/// Unique tools needed by the planned plugins, in plan order.
pub fn required_tools(descriptors: impl IntoIterator<Item = Descriptor>) -> Vec<ToolSpec> {
    let mut tools: Vec<ToolSpec> = Vec::new();
    for descriptor in descriptors {
        for tool in descriptor.requires.tools {
            if !tools.iter().any(|known| known.name == tool.name) {
                tools.push(*tool);
            }
        }
    }
    tools
}

/// Installs every tool into `dir`. `fetch` opens the download for a URL (HTTPS in
/// production, an in-memory body in tests) and returns a user-facing error on failure.
pub async fn provision_all<F, Fut, R>(
    tools: &[ToolSpec],
    arch: &str,
    dir: &Path,
    fetch: F,
) -> Provisioned
where
    F: Fn(&'static str) -> Fut,
    Fut: Future<Output = Result<R, String>>,
    R: AsyncRead + Unpin,
{
    let mut result = Provisioned::default();
    for tool in tools {
        let installed = match tool.asset_for(arch) {
            None => Err(format!("没有适用于 {arch} 的版本")),
            Some(asset) => match fetch(asset.url).await {
                Ok(body) => install(body, tool, asset, dir).await,
                Err(err) => Err(err),
            },
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
    result
}

/// Streams `body` into `dir/<name>`. The size limit is enforced while reading, the
/// SHA-256 is checked before the file becomes executable, and on any failure the
/// partial file is removed.
pub async fn install<R: AsyncRead + Unpin>(
    body: R,
    tool: &ToolSpec,
    asset: &ToolAsset,
    dir: &Path,
) -> Result<PathBuf, String> {
    let partial = dir.join(format!("{}.part", tool.name));
    let installed = dir.join(tool.name);
    let outcome = match write_verified(body, asset, &partial).await {
        Ok(()) => make_executable(&partial, &installed).await,
        Err(err) => Err(err),
    };
    if outcome.is_err() {
        let _ = fs::remove_file(&partial).await;
    }
    outcome.map(|()| installed)
}

async fn write_verified<R: AsyncRead + Unpin>(
    mut body: R,
    asset: &ToolAsset,
    path: &Path,
) -> Result<(), String> {
    use sha2::Digest;
    use std::fmt::Write;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    let write_error = |err: io::Error| format!("无法写入：{err}");
    let mut file = File::create(path).await.map_err(write_error)?;
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = body
            .read(&mut buffer)
            .await
            .map_err(|err| format!("下载中断：{err}"))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > asset.size {
            return Err(format!("大小超过声明的 {} 字节", asset.size));
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).await.map_err(write_error)?;
    }
    file.flush().await.map_err(write_error)?;
    if total != asset.size {
        return Err(format!(
            "大小不符：应为 {} 字节，实际 {total} 字节",
            asset.size
        ));
    }
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
