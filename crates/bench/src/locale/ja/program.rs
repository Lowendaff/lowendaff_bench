//! 日本語：コマンドラインヘルプ、`--list` 表示、および実行前後のエラー。

use std::fmt::Display;
use std::path::Path;

use super::Japanese;
use crate::locale::ProgramWords;

impl ProgramWords for Japanese {
    fn about(&self) -> String {
        "Linux サーバー向け統合ベンチマーク・回線診断ツール。root 権限で実行します\
         （オプション未指定時は対話型 TUI を起動）。"
            .to_owned()
    }

    fn help_all(&self) -> String {
        "非対話 CLI モードですべてのテスト項目を実行する".to_owned()
    }

    fn help_only(&self) -> String {
        "指定した項目のみ実行：カンマ区切りのカテゴリ（cpu,memory,disk,network,ip）、\
         項目 ID（cpu.bench,disk.fio など）、または all"
            .to_owned()
    }

    fn help_skip(&self) -> String {
        "指定した項目をスキップ：カンマ区切りのカテゴリ、項目 ID、または all\
         （cpu.bench,ip.media など）"
            .to_owned()
    }

    fn help_output(&self) -> String {
        "標準出力の代わりに指定ファイルへ書き出す（未指定時は画面出力に加え /tmp に保存）"
            .to_owned()
    }

    fn help_format(&self) -> String {
        "レポート形式：text、markdown（別名 md）、または json".to_owned()
    }

    fn help_lang(&self) -> String {
        "表示・レポート言語：en（既定）、zh-cn、zh-tw、または ja（JSON は常に英語）".to_owned()
    }

    fn help_work_dir(&self) -> String {
        "このディレクトリ配下に一時作業ディレクトリを作成し終了時に削除する（/tmp は不可）"
            .to_owned()
    }

    fn help_list(&self) -> String {
        "すべてのテスト項目を一覧表示して終了する".to_owned()
    }

    fn help_verbose(&self) -> String {
        "詳細ログを表示：ダウンロード URL・再試行、外部ツールの実行コマンド・終了コード".to_owned()
    }

    fn help_show_ip(&self) -> String {
        "レポート内のグローバル IP アドレスと逆引きホスト名を伏せ字にせず表示する".to_owned()
    }

    fn help_download_connections(&self) -> String {
        "1 ファイルのダウンロードで使用する最大同時接続数（1 〜 256）".to_owned()
    }

    fn help_socks5_proxy(&self) -> String {
        "ツール・リスト取得専用の SOCKS5 プロキシ（socks5:// または socks5h://；\
         測定通信には不使用）"
            .to_owned()
    }

    fn help_github_proxy(&self) -> String {
        "GitHub からのツール・リスト取得用ミラー URL プレフィックス\
         （例：https://ghfast.top/）"
            .to_owned()
    }

    fn invalid_socks5_proxy(&self) -> String {
        "無効な --socks5-proxy：socks5://[user:pass@]host[:port] または \
         socks5h://[user:pass@]host[:port] を指定してください"
            .to_owned()
    }

    fn invalid_github_proxy(&self) -> String {
        "無効な --github-proxy：認証情報を含まない http:// または https:// のベース URL\
         （例：https://ghfast.top/）を指定してください"
            .to_owned()
    }

    fn help_cpu_bench(&self) -> String {
        "cpu.bench のサブスイート：カンマ区切りの sysbench, 7zip, unixbench, hpl, \
         passmark, ycruncher, prime95, geekbench、または all"
            .to_owned()
    }

    fn help_countries(&self) -> String {
        "network.iperf3 で使用する測定国の ISO 3166 国コード（例：US,JP）または all".to_owned()
    }

    fn help_taier(&self) -> String {
        "network.taier のターゲットセレクタ：<拠点>[:<キャリア>[+<キャリア>...]]\
         （例：beijing:cu,guangdong/shenzhen:ct+cm,jiangsu:ct,all:cm）"
            .to_owned()
    }

    fn help_taier_list(&self) -> String {
        "利用可能な network.taier の地域（または指定した省の都市）を表示して終了する".to_owned()
    }

    fn help_taier_cities(&self) -> String {
        "network.taier の都市・省名・拠点 ID（例：Beijing,Shanghai,Jiangsu）または all".to_owned()
    }

    fn help_taier_operators(&self) -> String {
        "network.taier の通信キャリア：カンマ区切りの telecom, unicom, mobile, \
         cernet, cbn, other、または all"
            .to_owned()
    }

    fn help_no_cloudflare_udp(&self) -> String {
        "network.cloudflare 内の RFC 5389 STUN UDP パケットロス測定を省略する".to_owned()
    }

    fn help_inetspeed_threads(&self) -> String {
        "network.inetspeed マルチスレッド測定の並列スレッド数（1 〜 64）".to_owned()
    }

    fn help_inetspeed_endpoint(&self) -> String {
        "network.inetspeed で特定の Apple CDN IPv4/IPv6 エッジ IP を固定する（DoH 省略）".to_owned()
    }

    fn invalid_inetspeed_endpoint(&self, raw: &str) -> String {
        format!(
            "無効な --inetspeed-endpoint `{raw}`：グローバル IPv4 または \
             IPv6 アドレスを指定してください"
        )
    }

    fn help_trace_groups(&self) -> String {
        "network.trace の宛先グループ：カンマ区切りの services, china, \
         international、または all"
            .to_owned()
    }

    fn help_no_trace_tcp(&self) -> String {
        "network.trace 内の中国本土拠点向け TCP SYN 大容量パケット追跡を省略する".to_owned()
    }

    fn help_trace_packet_size(&self) -> String {
        "network.trace の中国本土拠点向け TCP 調査パケット長（64 〜 1500 バイト、既定 1024）"
            .to_owned()
    }

    fn help_media_groups(&self) -> String {
        "ip.media の地域グループ：カンマ区切りの global, asia, europe, \
         americas, oceania、または all"
            .to_owned()
    }

    fn help_no_dnsbl(&self) -> String {
        "ip.quality 内の DNSBL ブラックリスト照会を省略する".to_owned()
    }

    fn help_no_mail(&self) -> String {
        "ip.quality 内の外向き TCP 25 番メールポート疎通チェックを省略する".to_owned()
    }

    fn help_no_bgp_upload(&self) -> String {
        "text モードでの BGP トポロジー SVG の imageupload.app アップロードを省略する".to_owned()
    }

    fn tui_not_a_tty(&self) -> String {
        "対話型モードには端末（TTY）が必要です。CLI モードでは --all または --only <項目> \
         を指定してください"
            .to_owned()
    }

    fn list(&self, items: &[String]) -> String {
        items.join("、")
    }

    fn item_count(&self, count: usize) -> String {
        format!("テスト項目：計 {count} 件")
    }

    fn group_line(&self, title: &str, name: &str) -> String {
        format!("[{title}]（カテゴリ：{name}）")
    }

    fn item_line(
        &self,
        id: &str,
        name: &str,
        platforms: &[String],
        numa: bool,
        tools: &[String],
    ) -> String {
        let mut line = format!("  {id}  {name}");
        if !platforms.is_empty() {
            line.push_str(&format!("（対象：{}）", self.list(platforms)));
        }
        if numa {
            line.push_str("（対象：複数 NUMA ノード環境）");
        }
        if !tools.is_empty() {
            line.push_str(&format!("（ツール：{}）", self.list(tools)));
        }
        line
    }

    fn unknown_selection(&self, name: &str) -> String {
        format!("不明なテスト項目またはカテゴリ `{name}`")
    }

    fn output_unwritable(&self, path: &Path, error: &dyn Display) -> String {
        format!("レポートを {} に書き込めません：{error}", path.display())
    }

    fn overrides_invalid(&self, error: &dyn Display) -> String {
        format!("無効なプラグイン上書き設定：{error}")
    }

    fn config_unknown_item(&self, id: &str) -> String {
        format!("不明なテスト項目 `{id}` の設定が指定されました")
    }

    fn config_invalid(&self, id: &str, error: &dyn Display) -> String {
        format!("テスト項目 {id} の設定が無効です：{error}")
    }

    fn unknown_item(&self, id: &str) -> String {
        format!("不明なテスト項目 `{id}`")
    }

    fn not_root(&self) -> String {
        "lowendaff-bench は root 権限で実行してください（sudo を使用）".to_owned()
    }

    fn runtime_failed(&self, error: &dyn Display) -> String {
        format!("非同期ランタイムを起動できません：{error}")
    }

    fn host_failed(&self, error: &dyn Display) -> String {
        format!("ホスト情報を読み取れません：{error}")
    }

    fn network_failed(&self, error: &dyn Display) -> String {
        format!("ネットワーク疎通確認に失敗しました：{error}")
    }

    fn run_dir_failed(&self, dir: &Path, error: &dyn Display) -> String {
        format!(
            "{} 配下に一時作業ディレクトリを作成できません：{error}",
            dir.display()
        )
    }

    fn tools_dir_failed(&self, error: &dyn Display) -> String {
        format!("ツール用ディレクトリを作成できません：{error}")
    }

    fn http_client_failed(&self, error: &dyn Display) -> String {
        format!("HTTP クライアントを初期化できません：{error}")
    }

    fn tls_failed(&self, error: &dyn Display) -> String {
        format!("TLS を初期化できません：{error}")
    }

    fn panic_without_text(&self) -> String {
        "（メッセージなしのパニック）".to_owned()
    }
}
