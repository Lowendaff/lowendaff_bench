//! 日本語：ダウンロード理由および詳細ログ文言。

use std::fmt::Display;
use std::path::Path;

use bench_api::IpFamily;

use super::Japanese;
use crate::locale::Download;
use crate::locale::DownloadWords;
use crate::locale::Retry;

impl DownloadWords for Japanese {
    fn no_build(&self, arch: &str) -> String {
        format!("{arch} 向けのビルドがありません")
    }

    fn download_interrupted(&self) -> String {
        "ダウンロードが中断されました".to_owned()
    }

    fn no_ranges(&self, tool: &str, version: &str) -> String {
        format!("{tool} {version}：サーバーが範囲リクエスト非対応のため単一接続で取得します")
    }

    fn in_ranges(&self, tool: &str, version: &str, count: usize) -> String {
        format!("{tool} {version}：{count} 個のバイト範囲で並列ダウンロード中")
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
            Download::File { name } => format!("ファイル {name}"),
        };
        match range {
            Some((range, from)) => format!(
                "{download}：範囲 {range} の試行 {attempt} 回目が失敗；\
                 {wait} 秒後にバイト {from} から再開：{reason}"
            ),
            None => format!("{download}：試行 {attempt} 回目が失敗；{wait} 秒後に再試行：{reason}"),
        }
    }

    fn request_failed(&self, error: &dyn Display, hint: &str) -> String {
        format!("ダウンロード失敗：{error}{hint}")
    }

    fn http_status(&self, code: u16) -> String {
        format!("ダウンロード失敗：HTTP {code}")
    }

    fn wrong_range(&self) -> String {
        "ダウンロード失敗：要求とは異なるバイト範囲が返されました".to_owned()
    }

    fn ranges_stopped(&self) -> String {
        "ダウンロード失敗：サーバーが範囲リクエストの提供を途中で停止しました".to_owned()
    }

    fn broke_off(&self, error: &dyn Display) -> String {
        format!("ダウンロード切断：{error}")
    }

    fn too_large(&self, size: u64) -> String {
        format!("固定サイズ {size} バイトを超過しています")
    }

    fn file_too_large(&self, limit: u64) -> String {
        format!("上限 {limit} バイトを超過しています")
    }

    fn not_https(&self, url: &str) -> String {
        format!("HTTPS アドレスではありません：{url}")
    }

    fn family_unreachable(&self, family: IpFamily) -> String {
        let family = match family {
            IpFamily::Ipv4 => "IPv4",
            IpFamily::Ipv6 => "IPv6",
        };
        format!("未取得：このホストには {family} 接続がありません")
    }

    fn wrong_size(&self, size: u64, actual: u64) -> String {
        format!("サイズ不一致：期待値 {size} バイト、取得 {actual} バイト")
    }

    fn range_too_large(&self, length: u64) -> String {
        format!("範囲サイズ {length} バイトを超過しています")
    }

    fn range_wrong_size(&self, length: u64, actual: u64) -> String {
        format!("範囲サイズ不一致：期待値 {length} バイト、取得 {actual} バイト")
    }

    fn sha_mismatch(&self, pinned: &str, actual: &str) -> String {
        format!("SHA-256 不一致：期待値 {pinned}、実際 {actual}")
    }

    fn write_failed(&self, error: &dyn Display) -> String {
        format!("ダウンロードファイルを書き込めません：{error}")
    }

    fn read_failed(&self, error: &dyn Display) -> String {
        format!("ダウンロード済みファイルを読み取れません：{error}")
    }

    fn install_failed(&self, error: &dyn Display) -> String {
        format!("ツールを配置できません：{error}")
    }

    fn raw_sockets_linux_only(&self) -> String {
        "Raw ソケットが必要ですが、Linux でのみ付与されます".to_owned()
    }

    fn raw_sockets_no_new_privs(&self) -> String {
        "Raw ソケットが必要ですが、no_new_privs によりファイルケイパビリティが無効です".to_owned()
    }

    fn raw_sockets_bounded(&self) -> String {
        "Raw ソケットが必要ですが、コンテナ等の制限により CAP_NET_RAW がありません".to_owned()
    }

    fn raw_sockets_nosuid(&self) -> String {
        "Raw ソケットが必要ですが、作業ディレクトリが nosuid マウントされています；\
         --work-dir で別の場所を指定してください"
            .to_owned()
    }

    fn raw_sockets_failed(&self, error: &dyn Display) -> String {
        format!("Raw ソケット権限（CAP_NET_RAW）を付与できません：{error}")
    }

    fn unpack_failed(&self, error: &dyn Display) -> String {
        format!("展開失敗：{error}")
    }

    fn damaged_archive(&self, error: &dyn Display) -> String {
        format!("アーカイブ破損：{error}")
    }

    fn unsupported_entry(&self, path: &Path) -> String {
        format!(
            "アーカイブに非対応のエントリが含まれています：{}",
            path.display()
        )
    }

    fn no_executable(&self, executable: &str) -> String {
        format!("アーカイブ内に実行ファイル {executable} が見つかりません")
    }

    fn proxy_set(&self, option: &str) -> String {
        format!("；{option} が設定されています。プロキシの状態を確認してください")
    }

    fn proxy_unset(&self) -> String {
        "；プロキシ経由で取得するには --socks5-proxy または --github-proxy を指定してください"
            .to_owned()
    }
}
