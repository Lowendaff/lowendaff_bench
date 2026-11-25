//! 繁體中文：命令列說明、`--list` 輸出及執行前後錯誤提示。

use std::fmt::Display;
use std::path::Path;

use super::TraditionalChinese;
use crate::locale::ProgramWords;

impl ProgramWords for TraditionalChinese {
    fn about(&self) -> String {
        "Linux 伺服器綜合基準與網路檢測工具；需以 root 權限執行\
         （未指定其他參數時預設啟動互動式 TUI 介面）。"
            .to_owned()
    }

    fn help_all(&self) -> String {
        "在非互動命令列模式下執行全部測試項目".to_owned()
    }

    fn help_only(&self) -> String {
        "僅執行指定測試項目：英文逗號分隔的分類（cpu,memory,disk,network,ip）、\
         項目 ID（如 cpu.bench,disk.fio）或 all"
            .to_owned()
    }

    fn help_skip(&self) -> String {
        "略過指定測試項目：英文逗號分隔的分類、項目 ID 或 all\
         （如 cpu.bench,ip.media）"
            .to_owned()
    }

    fn help_output(&self) -> String {
        "將報告寫入指定檔案而非標準輸出（未指定時輸出至終端機並在 /tmp 儲存封存副本）".to_owned()
    }

    fn help_format(&self) -> String {
        "報告格式：text、markdown（或 md）或 json".to_owned()
    }

    fn help_lang(&self) -> String {
        "介面與報告語言：en（預設）、zh-cn、zh-tw 或 ja（JSON 始終保持英文）".to_owned()
    }

    fn help_work_dir(&self) -> String {
        "在該目錄下建立暫存執行目錄並在結束時自動清理（請避開 /tmp）".to_owned()
    }

    fn help_list(&self) -> String {
        "列出全部測試項目後結束".to_owned()
    }

    fn help_verbose(&self) -> String {
        "輸出詳細除錯日誌：下載位址與重試、子工具執行指令與結束碼".to_owned()
    }

    fn help_show_ip(&self) -> String {
        "在報告中顯示完整公網 IP 與反向解析主機名稱（預設僅顯示前綴遮罩形式）".to_owned()
    }

    fn help_download_connections(&self) -> String {
        "單一檔案下載的最大並行連線數（1 至 256）".to_owned()
    }

    fn help_socks5_proxy(&self) -> String {
        "僅用於工具與節點清單下載的 SOCKS5 代理（socks5:// 或 socks5h://；\
         絕不用於任何測速與探測）"
            .to_owned()
    }

    fn help_github_proxy(&self) -> String {
        "僅用於 GitHub 工具與清單下載的鏡像前綴 URL（例如 https://ghfast.top/）".to_owned()
    }

    fn invalid_socks5_proxy(&self) -> String {
        "無效的 --socks5-proxy：格式應為 socks5://[user:pass@]host[:port] \
         或 socks5h://[user:pass@]host[:port]"
            .to_owned()
    }

    fn invalid_github_proxy(&self) -> String {
        "無效的 --github-proxy：應為不含憑證的 http:// 或 https:// 前綴 URL\
         （例如 https://ghfast.top/）"
            .to_owned()
    }

    fn help_cpu_bench(&self) -> String {
        "cpu.bench 子套件：英文逗號分隔的 sysbench, 7zip, unixbench, hpl, \
         passmark, ycruncher, prime95, geekbench 或 all"
            .to_owned()
    }

    fn help_countries(&self) -> String {
        "network.iperf3 使用的測速國家/地區 ISO 3166 代碼（如 US,JP）或 all".to_owned()
    }

    fn help_taier(&self) -> String {
        "network.taier 節點選擇器：<地區>[:<電信商>[+<電信商>...]]\
         （如 beijing:cu,guangdong/shenzhen:ct+cm,jiangsu:ct,all:cm）"
            .to_owned()
    }

    fn help_taier_list(&self) -> String {
        "列出可用的 network.taier 省市區域（或指定省份下的城市）後結束".to_owned()
    }

    fn help_taier_cities(&self) -> String {
        "network.taier 測速城市、省份英文名稱或節點 ID（如 Beijing,Shanghai,Jiangsu）\
         或 all"
            .to_owned()
    }

    fn help_taier_operators(&self) -> String {
        "network.taier 測速電信商：英文逗號分隔的 telecom, unicom, mobile, \
         cernet, cbn, other 或 all"
            .to_owned()
    }

    fn help_no_cloudflare_udp(&self) -> String {
        "在 network.cloudflare 中略過 RFC 5389 STUN UDP 封包遺失探測".to_owned()
    }

    fn help_inetspeed_threads(&self) -> String {
        "network.inetspeed 多執行緒輪次的並行工作執行緒數（1 至 64）".to_owned()
    }

    fn help_inetspeed_endpoint(&self) -> String {
        "在 network.inetspeed 中固定指定 Apple CDN IPv4/IPv6 節點 IP（略過 DoH）".to_owned()
    }

    fn invalid_inetspeed_endpoint(&self, raw: &str) -> String {
        format!("無效的 --inetspeed-endpoint `{raw}`：應為公網 IPv4 或 IPv6 位址")
    }

    fn help_trace_groups(&self) -> String {
        "network.trace 路由目標分組：英文逗號分隔的 services, china, \
         international 或 all"
            .to_owned()
    }

    fn help_no_trace_tcp(&self) -> String {
        "在 network.trace 中略過中國大陸目標的 TCP SYN 大封包路由追蹤".to_owned()
    }

    fn help_trace_packet_size(&self) -> String {
        "network.trace 中國大陸目標 TCP 探測封包位元組大小（64 至 1500，預設 1024）".to_owned()
    }

    fn help_media_groups(&self) -> String {
        "ip.media 串流媒體區域分組：英文逗號分隔的 global, asia, europe, \
         americas, oceania 或 all"
            .to_owned()
    }

    fn help_no_dnsbl(&self) -> String {
        "在 ip.quality 中略過 DNSBL 黑名單查詢".to_owned()
    }

    fn help_no_mail(&self) -> String {
        "在 ip.quality 中略過出站 TCP 25 連接埠郵件連通性檢測".to_owned()
    }

    fn help_no_bgp_upload(&self) -> String {
        "在 text 模式下略過上傳 BGP 拓撲 SVG 圖至 imageupload.app".to_owned()
    }

    fn tui_not_a_tty(&self) -> String {
        "互動式介面需要終端機（TTY）；非互動環境請傳入 --all 或 --only <項目>".to_owned()
    }

    fn list(&self, items: &[String]) -> String {
        items.join("、")
    }

    fn item_count(&self, count: usize) -> String {
        format!("共 {count} 個測試項目")
    }

    fn group_line(&self, title: &str, name: &str) -> String {
        format!("[{title}]（分類：{name}）")
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
            line.push_str(&format!("（適用環境：{}）", self.list(platforms)));
        }
        if numa {
            line.push_str("（適用環境：多 NUMA 節點）");
        }
        if !tools.is_empty() {
            line.push_str(&format!("（外部工具：{}）", self.list(tools)));
        }
        line
    }

    fn unknown_selection(&self, name: &str) -> String {
        format!("未知的測試項目或分類 `{name}`")
    }

    fn output_unwritable(&self, path: &Path, error: &dyn Display) -> String {
        format!("無法將報告寫入 {}：{error}", path.display())
    }

    fn overrides_invalid(&self, error: &dyn Display) -> String {
        format!("無效的外掛覆寫參數：{error}")
    }

    fn config_unknown_item(&self, id: &str) -> String {
        format!("未知測試項目 `{id}` 的參數設定")
    }

    fn config_invalid(&self, id: &str, error: &dyn Display) -> String {
        format!("測試項目 {id} 參數設定無效：{error}")
    }

    fn unknown_item(&self, id: &str) -> String {
        format!("未知測試項目 `{id}`")
    }

    fn not_root(&self) -> String {
        "lowendaff-bench 需要以 root 權限執行（請使用 sudo）".to_owned()
    }

    fn runtime_failed(&self, error: &dyn Display) -> String {
        format!("無法啟動非同步執行階段：{error}")
    }

    fn host_failed(&self, error: &dyn Display) -> String {
        format!("無法讀取主機硬體資訊：{error}")
    }

    fn network_failed(&self, error: &dyn Display) -> String {
        format!("無法完成網路連通性探測：{error}")
    }

    fn run_dir_failed(&self, dir: &Path, error: &dyn Display) -> String {
        format!("無法在 {} 下建立暫存執行目錄：{error}", dir.display())
    }

    fn tools_dir_failed(&self, error: &dyn Display) -> String {
        format!("無法建立工具目錄：{error}")
    }

    fn http_client_failed(&self, error: &dyn Display) -> String {
        format!("無法初始化 HTTP 用戶端：{error}")
    }

    fn tls_failed(&self, error: &dyn Display) -> String {
        format!("無法初始化 TLS 設定：{error}")
    }

    fn panic_without_text(&self) -> String {
        "（無文字訊息的異常崩潰）".to_owned()
    }
}
