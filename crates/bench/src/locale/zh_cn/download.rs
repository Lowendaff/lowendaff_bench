//! 简体中文：下载状态、校验失败原因与详细日志。

use std::fmt::Display;
use std::path::Path;

use bench_api::IpFamily;

use super::SimplifiedChinese;
use crate::locale::Download;
use crate::locale::DownloadWords;
use crate::locale::Retry;

impl DownloadWords for SimplifiedChinese {
    fn no_build(&self, arch: &str) -> String {
        format!("暂无适用于 {arch} 架构的构建产物")
    }

    fn download_interrupted(&self) -> String {
        "下载已中断".to_owned()
    }

    fn no_ranges(&self, tool: &str, version: &str) -> String {
        format!("{tool} {version}：服务器不支持分段请求，改用单连接下载")
    }

    fn in_ranges(&self, tool: &str, version: &str, count: usize) -> String {
        format!("{tool} {version}：启用 {count} 个并发分段下载")
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
            Download::File { name } => format!("文件 {name}"),
        };
        match range {
            Some((range, from)) => format!(
                "{download}：字节范围 {range} 第 {attempt} 次尝试失败；\
                 {wait} 秒后从字节 {from} 续传：{reason}"
            ),
            None => format!("{download}：第 {attempt} 次尝试失败；{wait} 秒后重试：{reason}"),
        }
    }

    fn request_failed(&self, error: &dyn Display, hint: &str) -> String {
        format!("下载失败：{error}{hint}")
    }

    fn http_status(&self, code: u16) -> String {
        format!("下载失败：HTTP {code}")
    }

    fn wrong_range(&self) -> String {
        "下载失败：服务器返回的字节范围与请求不符".to_owned()
    }

    fn ranges_stopped(&self) -> String {
        "下载失败：服务器中途停止响应字节范围请求".to_owned()
    }

    fn broke_off(&self, error: &dyn Display) -> String {
        format!("下载连接中断：{error}")
    }

    fn too_large(&self, size: u64) -> String {
        format!("文件超出锁定的 {size} 字节上限")
    }

    fn file_too_large(&self, limit: u64) -> String {
        format!("文件大小超过 {limit} 字节上限")
    }

    fn not_https(&self, url: &str) -> String {
        format!("非 HTTPS 下载地址：{url}")
    }

    fn family_unreachable(&self, family: IpFamily) -> String {
        let family = match family {
            IpFamily::Ipv4 => "IPv4",
            IpFamily::Ipv6 => "IPv6",
        };
        format!("未拉取：当前主机无 {family} 网络连接")
    }

    fn wrong_size(&self, size: u64, actual: u64) -> String {
        format!("文件大小不匹配：预期 {size} 字节，实际下载 {actual} 字节")
    }

    fn range_too_large(&self, length: u64) -> String {
        format!("分段响应超出预期的 {length} 字节")
    }

    fn range_wrong_size(&self, length: u64, actual: u64) -> String {
        format!("分段大小不匹配：预期 {length} 字节，实际返回 {actual} 字节")
    }

    fn sha_mismatch(&self, pinned: &str, actual: &str) -> String {
        format!("SHA-256 摘要不匹配：预期 {pinned}，实际为 {actual}")
    }

    fn write_failed(&self, error: &dyn Display) -> String {
        format!("无法写入下载文件：{error}")
    }

    fn read_failed(&self, error: &dyn Display) -> String {
        format!("无法读取已下载文件：{error}")
    }

    fn install_failed(&self, error: &dyn Display) -> String {
        format!("无法安装工具文件：{error}")
    }

    fn raw_sockets_linux_only(&self) -> String {
        "该工具需要原始套接字（raw socket），仅在 Linux 上受支持".to_owned()
    }

    fn raw_sockets_no_new_privs(&self) -> String {
        "该工具需要原始套接字，但进程启用了 no_new_privs，导致内核忽略文件能力".to_owned()
    }

    fn raw_sockets_bounded(&self) -> String {
        "该工具需要原始套接字，但当前容器或进程边界集已剥离 CAP_NET_RAW 能力".to_owned()
    }

    fn raw_sockets_nosuid(&self) -> String {
        "该工具需要原始套接字，但运行目录所在文件系统挂载了 nosuid；\
         请用 --work-dir 指定其他目录"
            .to_owned()
    }

    fn raw_sockets_failed(&self, error: &dyn Display) -> String {
        format!("无法授予原始套接字能力（CAP_NET_RAW）：{error}")
    }

    fn unpack_failed(&self, error: &dyn Display) -> String {
        format!("解压失败：{error}")
    }

    fn damaged_archive(&self, error: &dyn Display) -> String {
        format!("压缩包已损坏：{error}")
    }

    fn unsupported_entry(&self, path: &Path) -> String {
        format!("压缩包含有不支持的条目：{}", path.display())
    }

    fn no_executable(&self, executable: &str) -> String {
        format!("压缩包中未找到可执行文件 {executable}")
    }

    fn proxy_set(&self, option: &str) -> String {
        format!("；当前已设置 {option}，请检查代理是否可用")
    }

    fn proxy_unset(&self) -> String {
        "；可通过 --socks5-proxy 或 --github-proxy 配置下载代理".to_owned()
    }
}
