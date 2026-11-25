//! 繁體中文：下載狀態、驗證失敗原因與詳細日誌。

use std::fmt::Display;
use std::path::Path;

use bench_api::IpFamily;

use super::TraditionalChinese;
use crate::locale::Download;
use crate::locale::DownloadWords;
use crate::locale::Retry;

impl DownloadWords for TraditionalChinese {
    fn no_build(&self, arch: &str) -> String {
        format!("暫無適用於 {arch} 架構的建置產物")
    }

    fn download_interrupted(&self) -> String {
        "下載已中斷".to_owned()
    }

    fn no_ranges(&self, tool: &str, version: &str) -> String {
        format!("{tool} {version}：伺服器不支援分段請求，改用單連線下載")
    }

    fn in_ranges(&self, tool: &str, version: &str, count: usize) -> String {
        format!("{tool} {version}：啟用 {count} 個並行分段下載")
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
            Download::File { name } => format!("檔案 {name}"),
        };
        match range {
            Some((range, from)) => format!(
                "{download}：位元組範圍 {range} 第 {attempt} 次嘗試失敗；\
                 {wait} 秒後從位元組 {from} 續傳：{reason}"
            ),
            None => format!("{download}：第 {attempt} 次嘗試失敗；{wait} 秒後重試：{reason}"),
        }
    }

    fn request_failed(&self, error: &dyn Display, hint: &str) -> String {
        format!("下載失敗：{error}{hint}")
    }

    fn http_status(&self, code: u16) -> String {
        format!("下載失敗：HTTP {code}")
    }

    fn wrong_range(&self) -> String {
        "下載失敗：伺服器回傳的位元組範圍與請求不符".to_owned()
    }

    fn ranges_stopped(&self) -> String {
        "下載失敗：伺服器中途停止回應位元組範圍請求".to_owned()
    }

    fn broke_off(&self, error: &dyn Display) -> String {
        format!("下載連線中斷：{error}")
    }

    fn too_large(&self, size: u64) -> String {
        format!("檔案超出鎖定的 {size} 位元組上限")
    }

    fn file_too_large(&self, limit: u64) -> String {
        format!("檔案大小超過 {limit} 位元組上限")
    }

    fn not_https(&self, url: &str) -> String {
        format!("非 HTTPS 下載位址：{url}")
    }

    fn family_unreachable(&self, family: IpFamily) -> String {
        let family = match family {
            IpFamily::Ipv4 => "IPv4",
            IpFamily::Ipv6 => "IPv6",
        };
        format!("未擷取：目前主機無 {family} 網路連線")
    }

    fn wrong_size(&self, size: u64, actual: u64) -> String {
        format!("檔案大小不符：預期 {size} 位元組，實際下載 {actual} 位元組")
    }

    fn range_too_large(&self, length: u64) -> String {
        format!("分段回應超出預期的 {length} 位元組")
    }

    fn range_wrong_size(&self, length: u64, actual: u64) -> String {
        format!("分段大小不符：預期 {length} 位元組，實際回傳 {actual} 位元組")
    }

    fn sha_mismatch(&self, pinned: &str, actual: &str) -> String {
        format!("SHA-256 雜湊值不符：預期 {pinned}，實際為 {actual}")
    }

    fn write_failed(&self, error: &dyn Display) -> String {
        format!("無法寫入下載檔案：{error}")
    }

    fn read_failed(&self, error: &dyn Display) -> String {
        format!("無法讀取已下載檔案：{error}")
    }

    fn install_failed(&self, error: &dyn Display) -> String {
        format!("無法安裝工具檔案：{error}")
    }

    fn raw_sockets_linux_only(&self) -> String {
        "該工具需要原始通訊端（raw socket），僅在 Linux 上支援".to_owned()
    }

    fn raw_sockets_no_new_privs(&self) -> String {
        "該工具需要原始通訊端，但行程啟用了 no_new_privs，導致核心忽略檔案能力".to_owned()
    }

    fn raw_sockets_bounded(&self) -> String {
        "該工具需要原始通訊端，但目前容器或行程邊界集已移除 CAP_NET_RAW 能力".to_owned()
    }

    fn raw_sockets_nosuid(&self) -> String {
        "該工具需要原始通訊端，但執行目錄所在檔案系統掛載了 nosuid；\
         請用 --work-dir 指定其他目錄"
            .to_owned()
    }

    fn raw_sockets_failed(&self, error: &dyn Display) -> String {
        format!("無法授予原始通訊端能力（CAP_NET_RAW）：{error}")
    }

    fn unpack_failed(&self, error: &dyn Display) -> String {
        format!("解壓縮失敗：{error}")
    }

    fn damaged_archive(&self, error: &dyn Display) -> String {
        format!("壓縮檔已損毀：{error}")
    }

    fn unsupported_entry(&self, path: &Path) -> String {
        format!("壓縮檔含有不支援的項目：{}", path.display())
    }

    fn no_executable(&self, executable: &str) -> String {
        format!("壓縮檔中找不到可執行檔 {executable}")
    }

    fn proxy_set(&self, option: &str) -> String {
        format!("；目前已設定 {option}，請檢查代理伺服器是否可用")
    }

    fn proxy_unset(&self) -> String {
        "；可透過 --socks5-proxy 或 --github-proxy 設定下載代理".to_owned()
    }
}
