//! Dev check: download a URL with the provisioning client and print the byte count.
//! Shows that HTTPS works on a host without `ca-certificates` (bundled roots).
//!
//! `cargo run --example fetch -- https://example.com/`

use bench::fetch::Fetcher;
use tokio::io::AsyncReadExt;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), String> {
    let url = std::env::args().nth(1).ok_or("用法：fetch <url>")?;
    let mut body = Fetcher::new()?.open(&url).await?;
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)
        .await
        .map_err(|err| format!("读取失败：{err}"))?;
    println!("{url}：{} 字节", bytes.len());
    Ok(())
}
