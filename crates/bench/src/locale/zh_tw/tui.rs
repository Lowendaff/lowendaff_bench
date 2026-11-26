//! 繁體中文：互動式終端機介面（TUI）文案。

use std::path::Path;

use super::TraditionalChinese;
use crate::locale::TuiWords;

impl TuiWords for TraditionalChinese {
    fn tui_config_title(&self, version: &str) -> String {
        format!("lowendaff-bench {version}  互動式設定")
    }

    fn tui_running_title(&self, version: &str, elapsed: &str) -> String {
        format!("lowendaff-bench {version}  執行中 {elapsed}")
    }

    fn tui_results_title(&self, version: &str) -> String {
        format!("lowendaff-bench {version}  報告檢視器")
    }

    fn tui_enabled_badge(&self, enabled: usize, total: usize) -> String {
        format!("已啟用 {enabled}/{total}")
    }

    fn tui_terminal_too_small(&self, cols: u16, rows: u16) -> String {
        format!("終端機視窗（{cols}x{rows}）過小；請至少調整至 60x16 或按 q 離開")
    }

    fn tui_items_pane(&self) -> &'static str {
        "測試項目與全域選項"
    }

    fn tui_start_action(&self, enabled: usize) -> String {
        format!("開始執行測試（已選 {enabled} 項）")
    }

    fn tui_global_row(&self) -> &'static str {
        "執行與報告全域選項"
    }

    fn tui_overview_pane(&self) -> &'static str {
        "執行概覽"
    }

    fn tui_global_pane(&self) -> &'static str {
        "全域執行與報告選項"
    }

    fn tui_params_pane(&self, id: &str) -> String {
        format!("項目參數設定 ({id})")
    }

    fn tui_overview_lines(
        &self,
        enabled: usize,
        total: usize,
        customized: usize,
        format: &str,
        output: &str,
        work_dir: &str,
    ) -> Vec<String> {
        vec![
            format!("已選測試項目：{enabled} / {total}"),
            format!("已自訂項目：  {customized}"),
            format!("報告格式：    {format}"),
            format!("輸出檔案：    {output}"),
            format!("工作目錄：    {work_dir}"),
            String::new(),
            "按 Enter 或 r 依標準順序啟動所選測試項目。".to_owned(),
            "按向下鍵開關單項，或按向右/Tab 調整各單項與全域參數。".to_owned(),
            "快捷鍵：g 切換整組分類；a 全選/全不選；d 恢復預設；L 切換語言。".to_owned(),
        ]
    }

    fn tui_item_meta(&self, group: &str, timeout_mins: u64) -> String {
        format!("分類：{group}  |  逾時上限：{timeout_mins} 分鐘")
    }

    fn tui_item_requires(&self, requirements: &str) -> String {
        format!("執行需求：{requirements}")
    }

    fn tui_any_host(&self) -> &'static str {
        "所有 Linux 主機"
    }

    fn tui_req_ipv4(&self) -> &'static str {
        "IPv4"
    }

    fn tui_req_ipv6(&self) -> &'static str {
        "IPv6"
    }

    fn tui_req_internet(&self) -> &'static str {
        "公網連線"
    }

    fn tui_req_numa(&self) -> &'static str {
        "多 NUMA 節點"
    }

    fn tui_req_glibc(&self, major: u32, minor: u32) -> String {
        format!("glibc >= {major}.{minor}")
    }

    fn tui_fixed_params_note(&self) -> &'static str {
        "此項目採用標準化固定參數，確保不同伺服器之間的測試分數可直接橫向比較。"
    }

    fn tui_yes(&self) -> &'static str {
        "是"
    }

    fn tui_no(&self) -> &'static str {
        "否"
    }

    fn tui_field_enabled(&self) -> &'static str {
        "啟用狀態"
    }

    fn tui_field_cpu_suite(&self, suite: &str) -> String {
        let name = match suite {
            "sysbench" => "sysbench（質數篩選）",
            "7zip" => "7-Zip（LZMA2 MIPS）",
            "unixbench" => "UnixBench（12 項綜合指數）",
            "hpl" => "HPL（OpenBLAS GFLOPS）",
            "passmark" => "PassMark（CPU Mark）",
            "ycruncher" => "y-cruncher（圓周率 Pi，僅 x86_64）",
            "prime95" => "Prime95（Small FFT 燒機，僅 x86_64）",
            "geekbench" => "Geekbench 7",
            other => other,
        };
        format!("子套件：{name}")
    }

    fn tui_field_countries(&self) -> &'static str {
        "國家/地區 (ISO / all)"
    }

    fn tui_field_servers(&self) -> &'static str {
        "指定節點 ID（覆寫）"
    }

    fn tui_field_seconds(&self) -> &'static str {
        "單向測速秒數"
    }

    fn tui_field_streams(&self) -> &'static str {
        "並行串流數"
    }

    fn tui_field_rate(&self) -> &'static str {
        "單流限速 (Mbit/s)"
    }

    fn tui_field_taier_browse(&self) -> &'static str {
        "選擇測速節點（樹狀）"
    }

    fn tui_field_taier_targets(&self) -> &'static str {
        "目標選擇器"
    }

    fn tui_taier_browse_value(&self, selections: &str) -> String {
        let count = selections
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .count();
        if count == 0 {
            "[Enter] 瀏覽省市與電信商節點（預設北上廣三網）".to_owned()
        } else {
            format!("[Enter] 瀏覽省市與電信商節點（已選 {count} 組選擇器）")
        }
    }

    fn tui_taier_picker_title(&self) -> &'static str {
        "中國三網測速節點選擇器 (network.taier)"
    }

    fn tui_taier_picker_root(&self) -> &'static str {
        "全部區域"
    }

    fn tui_taier_picker_loading(&self) -> &'static str {
        "正在擷取中國信通院全球網測節點清單...（按 Esc 取消）"
    }

    fn tui_taier_picker_error_hint(&self) -> &'static str {
        "Enter / r：重試擷取  |  Esc：關閉"
    }

    fn tui_taier_picker_tip(&self) -> &'static str {
        "直轄市直接展開電信商，省份展開下轄城市；按 / 搜尋過濾。"
    }

    fn tui_taier_picker_keys(&self) -> &'static str {
        "Space:勾選  Enter/右:展開  左:返回  /:搜尋  a:全選  d:預設  c:清空  s/Esc:套用  q:取消"
    }

    fn tui_taier_operator_label(&self, code: &str) -> &'static str {
        match code {
            "telecom" => "中國電信 (ct)",
            "unicom" => "中國聯通 (cu)",
            "mobile" => "中國移動 (cm)",
            "cernet" => "教育網 CERNET (edu)",
            "cbn" => "中國廣電 (cbn)",
            "other" => "其他 / 境外 (other)",
            _ => "電信商",
        }
    }

    fn tui_taier_picker_empty_err(&self) -> String {
        "請至少勾選一個電信商節點（按 d 恢復預設，或按 q 取消）".to_owned()
    }

    fn tui_field_cloudflare_concurrency(&self) -> &'static str {
        "並行工作數"
    }

    fn tui_field_cloudflare_udp(&self) -> &'static str {
        "STUN UDP 封包遺失探測"
    }

    fn tui_field_inetspeed_threads(&self) -> &'static str {
        "多執行緒工作數"
    }

    fn tui_field_inetspeed_timeout(&self) -> &'static str {
        "單執行緒逾時（秒）"
    }

    fn tui_field_inetspeed_max(&self) -> &'static str {
        "單執行緒流量上限"
    }

    fn tui_field_inetspeed_latency_count(&self) -> &'static str {
        "閒置延遲取樣數"
    }

    fn tui_field_inetspeed_endpoint(&self) -> &'static str {
        "固定節點 IP"
    }

    fn tui_field_tcping_group(&self, group: &str) -> String {
        format!("探測分組：{group}")
    }

    fn tui_field_tcping_all(&self) -> &'static str {
        "展開全部 31 省與全球節點"
    }

    fn tui_field_tcping_count(&self) -> &'static str {
        "單節點握手探測次數"
    }

    fn tui_field_trace_group(&self, group: &str) -> String {
        format!("路由分組：{group}")
    }

    fn tui_field_targets(&self) -> &'static str {
        "指定目標 ID（覆寫）"
    }

    fn tui_field_geo(&self) -> &'static str {
        "跳點 ASN 與地理位置查詢"
    }

    fn tui_field_trace_tcp(&self) -> &'static str {
        "中國大陸 TCP 大封包追蹤"
    }

    fn tui_field_trace_packet_size(&self) -> &'static str {
        "TCP 探測封包大小（位元組）"
    }

    fn tui_field_bgp_upload(&self) -> &'static str {
        "上傳拓撲圖產生連結"
    }

    fn tui_field_dnsbl(&self) -> &'static str {
        "DNSBL 黑名單查詢 (25)"
    }

    fn tui_field_mail(&self) -> &'static str {
        "25 連接埠郵局連通性"
    }

    fn tui_field_media_group(&self, group: &str) -> String {
        format!("檢測區域：{group}")
    }

    fn tui_field_services(&self) -> &'static str {
        "指定平台 ID（覆寫）"
    }

    fn tui_field_language(&self) -> &'static str {
        "介面與報告語言"
    }

    fn tui_lang_picker_title(&self) -> &'static str {
        "選擇介面與報告語言"
    }

    fn tui_lang_picker_keys(&self) -> &'static str {
        "上下鍵: 移動  Enter/Space: 確認切換  Esc/q: 取消"
    }

    fn tui_field_format(&self) -> &'static str {
        "報告格式"
    }

    fn tui_field_output(&self) -> &'static str {
        "儲存報告至檔案"
    }

    fn tui_field_work_dir(&self) -> &'static str {
        "工作目錄"
    }

    fn tui_field_show_ip(&self) -> &'static str {
        "公網 IP 顯示方式"
    }

    fn tui_field_connections(&self) -> &'static str {
        "下載並行連線數"
    }

    fn tui_field_socks5_proxy(&self) -> &'static str {
        "SOCKS5 下載代理"
    }

    fn tui_field_github_proxy(&self) -> &'static str {
        "GitHub 鏡像代理前綴"
    }

    fn tui_field_verbose(&self) -> &'static str {
        "詳細除錯日誌"
    }

    fn tui_default_locations(&self) -> &'static str {
        "預設（5 個精選地區）"
    }

    fn tui_default_taier_cities(&self) -> &'static str {
        "預設（北京、上海、廣州）"
    }

    fn tui_default_inetspeed_endpoint(&self) -> &'static str {
        "自動（雙 DoH 優選最低 RTT）"
    }

    fn tui_default_none(&self) -> &'static str {
        "無"
    }

    fn tui_unlimited(&self) -> &'static str {
        "不限速"
    }

    fn tui_default_cpu_note(&self) -> &'static str {
        "y-cruncher 與 Prime95 預設為選用（僅支援 x86_64，ARM64 自動略過）。"
    }

    fn tui_default_taier_note(&self) -> &'static str {
        "預設測試北京、上海、廣州的電信、聯通、移動三網節點。"
    }

    fn tui_default_cloudflare_note(&self) -> &'static str {
        "測量 speed.cloudflare.com 雙棧吞吐、緩衝膨脹評級與 STUN UDP 封包遺失。"
    }

    fn tui_default_inetspeed_note(&self) -> &'static str {
        "透過雙 DoH 優選最佳 mensura.cdn-apple.com 節點並執行單執行緒與多執行緒測速。"
    }

    fn tui_default_tcping_note(&self) -> &'static str {
        "預設探測精選三網省會、CN2/9929/CMIN2 精品網與全球核心樞紐（可展開 31 省全量節點）。"
    }

    fn tui_default_trace_note(&self) -> &'static str {
        "未勾選分組時預設測試最近影片快取與精選電信商線路。"
    }

    fn tui_default_bgp_note(&self) -> &'static str {
        "在 text 模式下上傳遮罩後的 AS 路徑 SVG 至 imageupload.app 產生圖片連結。"
    }

    fn tui_default_media_note(&self) -> &'static str {
        "未勾選區域時預設檢測全球五大區域全部平台。"
    }

    fn tui_masked(&self) -> &'static str {
        "否（預設遮罩）"
    }

    fn tui_unmasked(&self) -> &'static str {
        "是（顯示完整 IP）"
    }

    fn tui_edit_title(&self, field: &str) -> String {
        format!("編輯：{field}")
    }

    fn tui_edit_hint(&self) -> &'static str {
        "Enter: 確認  |  Ctrl-U: 清空（恢復預設）  |  Esc: 取消"
    }

    fn tui_no_items_selected(&self) -> String {
        "請至少啟用一個測試項目後再開始執行".to_owned()
    }

    fn tui_no_cpu_suites_selected(&self) -> String {
        "請在 cpu.bench 中至少勾選一個 CPU 子套件（或停用 cpu.bench）".to_owned()
    }

    fn tui_invalid_number(&self, field: &str, min: u32, max: u32) -> String {
        format!("{field} 必須是 {min} 到 {max} 之間的整數")
    }

    fn tui_empty_work_dir(&self) -> String {
        "工作目錄不可為空（目前目錄請填 .）".to_owned()
    }

    fn tui_all_items_and_options(&self) -> &'static str {
        "全部項目與執行選項"
    }

    fn tui_reset_done(&self, target: &str) -> String {
        format!("已將 {target} 恢復為預設設定")
    }

    fn tui_gauge_completed(&self, done: usize, total: usize) -> String {
        format!("已完成 {done} / {total}")
    }

    fn tui_taier_picker_badge(&self, selected: usize, est_secs: u64) -> String {
        let est_text = if est_secs >= 60 {
            format!("~{}分 {:02}秒", est_secs / 60, est_secs % 60)
        } else {
            format!("~{est_secs}秒")
        };
        format!("已選 {selected} 組 ({est_text})")
    }

    fn tui_taier_filter_prefix(&self) -> &'static str {
        "搜尋過濾："
    }

    fn tui_taier_filter_active(&self, filter: &str) -> String {
        format!("目前過濾：\"{filter}\"（/: 編輯，Esc: 清除）")
    }

    fn tui_taier_no_matches(&self) -> &'static str {
        "無相符地區（按 Esc 清除過濾條件）"
    }

    fn tui_taier_targets_preview(&self, targets: Option<&str>) -> String {
        match targets {
            Some(text) => format!("已選目標：{text}"),
            None => "已選目標：（未選擇）".to_owned(),
        }
    }

    fn tui_taier_region_meta(&self, municipality: bool, overseas: bool, cities: usize) -> String {
        if municipality {
            "  （直轄市）".to_owned()
        } else if overseas {
            format!("  （{cities} 個境外節點）")
        } else {
            format!("  （{cities} 個城市）")
        }
    }

    fn tui_taier_node_count(&self, count: usize) -> String {
        if count == 1 {
            "1 個節點".to_owned()
        } else {
            format!("{count} 個備援節點")
        }
    }

    fn tui_keys_config_list(&self) -> &'static str {
        "上下:移動  Space:開關  右/Tab:參數  g:整組  a:全選  d:重設  L:語言  r:執行  q:離開"
    }

    fn tui_keys_config_params(&self) -> &'static str {
        "上下:選項  Space/Enter:切換/編輯  左右:調整  Esc/Tab:返回  d:重設  L:語言  r:執行"
    }

    fn tui_progress_pane(&self, done: usize, total: usize) -> String {
        format!("測試進度 ({done}/{total})")
    }

    fn tui_log_pane(&self, following: bool) -> &'static str {
        if following {
            "即時事件日誌（自動跟隨）"
        } else {
            "即時事件日誌（已暫停 - 按 End 跟隨最新）"
        }
    }

    fn tui_collecting_host(&self) -> &'static str {
        "正在蒐集主機硬體資訊並探測 IPv4/IPv6 連通性..."
    }

    fn tui_state_queued(&self) -> &'static str {
        "排隊中"
    }

    fn tui_state_running(&self) -> &'static str {
        "執行中"
    }

    fn tui_state_not_applicable(&self) -> &'static str {
        "目前主機不適用"
    }

    fn tui_stopping_banner(&self) -> &'static str {
        "正在安全停止目前測試以產生已完成部分報告... 再次按 Ctrl-C 立即離開"
    }

    fn tui_keys_running(&self) -> &'static str {
        "上下/PgUp/PgDn: 捲動日誌  End: 跟隨最新  Ctrl-C: 停止並檢視已完成報告"
    }

    fn tui_sections_pane(&self) -> &'static str {
        "章節導覽"
    }

    fn tui_section_top(&self) -> &'static str {
        "頂部 / 概覽"
    }

    fn tui_report_pane(&self, format: &str, wrapped: bool, show_ip: bool) -> String {
        let wrap = if format == "text" {
            if wrapped {
                "，自動換行"
            } else {
                "，不換行"
            }
        } else {
            ""
        };
        let ip = if show_ip { "完整 IP" } else { "遮罩 IP" };
        format!("測試報告（{format}{wrap}，{ip}）")
    }

    fn tui_save_prompt_title(&self) -> &'static str {
        "儲存報告至檔案"
    }

    fn tui_saved_to(&self, path: &Path) -> String {
        format!("報告已儲存至 {}", path.display())
    }

    fn tui_keys_results(&self) -> &'static str {
        "方向鍵/PgUp/PgDn:捲動  [/]:章節  b:側欄  f:格式  w:換行  i:IP  L:語言  s:儲存  \
         p:列印離開  n:新測  q:離開"
    }
}
