//! 日本語：対話型ターミナル画面（TUI）の文言。

use std::path::Path;

use super::Japanese;
use crate::locale::TuiWords;

impl TuiWords for Japanese {
    fn tui_config_title(&self, version: &str) -> String {
        format!("lowendaff-bench {version}  対話型セットアップ")
    }

    fn tui_running_title(&self, version: &str, elapsed: &str) -> String {
        format!("lowendaff-bench {version}  実行中 {elapsed}")
    }

    fn tui_results_title(&self, version: &str) -> String {
        format!("lowendaff-bench {version}  レポートビューア")
    }

    fn tui_enabled_badge(&self, enabled: usize, total: usize) -> String {
        format!("{enabled}/{total} 有効")
    }

    fn tui_terminal_too_small(&self, cols: u16, rows: u16) -> String {
        format!(
            "端末サイズ（{cols}x{rows}）が小さすぎます。60x16 以上に広げるか q で終了してください"
        )
    }

    fn tui_items_pane(&self) -> &'static str {
        "テスト項目・全体オプション"
    }

    fn tui_start_action(&self, enabled: usize) -> String {
        format!("ベンチマーク開始（{enabled} 項目選択中）")
    }

    fn tui_global_row(&self) -> &'static str {
        "実行・レポート全体設定"
    }

    fn tui_overview_pane(&self) -> &'static str {
        "実行概要"
    }

    fn tui_global_pane(&self) -> &'static str {
        "実行・レポート全体オプション"
    }

    fn tui_params_pane(&self, id: &str) -> String {
        format!("項目パラメータ設定 ({id})")
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
            format!("選択中の項目：    {enabled} / {total}"),
            format!("カスタム設定済：  {customized}"),
            format!("レポート形式：    {format}"),
            format!("出力ファイル：    {output}"),
            format!("作業ディレクトリ：{work_dir}"),
            String::new(),
            "Enter または r キーで選択した項目を標準順に実行します。".to_owned(),
            "下キーで項目の有効/無効切替、右キー/Tab で個別・全体設定を編集します。".to_owned(),
            "ショートカット：g カテゴリ切替 / a 全選択 / d 初期化 / L 言語切替。".to_owned(),
        ]
    }

    fn tui_item_meta(&self, group: &str, timeout_mins: u64) -> String {
        format!("カテゴリ：{group}  |  制限時間：{timeout_mins} 分")
    }

    fn tui_item_requires(&self, requirements: &str) -> String {
        format!("必要条件：{requirements}")
    }

    fn tui_any_host(&self) -> &'static str {
        "すべての Linux ホスト"
    }

    fn tui_req_ipv4(&self) -> &'static str {
        "IPv4"
    }

    fn tui_req_ipv6(&self) -> &'static str {
        "IPv6"
    }

    fn tui_req_internet(&self) -> &'static str {
        "インターネット接続"
    }

    fn tui_req_numa(&self) -> &'static str {
        "複数 NUMA ノード"
    }

    fn tui_req_glibc(&self, major: u32, minor: u32) -> String {
        format!("glibc >= {major}.{minor}")
    }

    fn tui_fixed_params_note(&self) -> &'static str {
        "サーバー間でスコアを公平に比較できるよう、標準の固定条件で測定します。"
    }

    fn tui_yes(&self) -> &'static str {
        "有効"
    }

    fn tui_no(&self) -> &'static str {
        "無効"
    }

    fn tui_field_enabled(&self) -> &'static str {
        "有効化"
    }

    fn tui_field_cpu_suite(&self, suite: &str) -> String {
        let name = match suite {
            "sysbench" => "sysbench（素数計算）",
            "7zip" => "7-Zip（LZMA2 MIPS）",
            "unixbench" => "UnixBench（12 種総合指数）",
            "hpl" => "HPL（OpenBLAS GFLOPS）",
            "passmark" => "PassMark（CPU Mark）",
            "ycruncher" => "y-cruncher（円周率 Pi、x86_64 専用）",
            "prime95" => "Prime95（Small FFT 負荷、x86_64 専用）",
            "geekbench" => "Geekbench 7",
            other => other,
        };
        format!("スイート：{name}")
    }

    fn tui_field_countries(&self) -> &'static str {
        "対象国 (ISO / all)"
    }

    fn tui_field_servers(&self) -> &'static str {
        "サーバー ID 指定"
    }

    fn tui_field_seconds(&self) -> &'static str {
        "片道測定秒数 (秒)"
    }

    fn tui_field_streams(&self) -> &'static str {
        "並列ストリーム数"
    }

    fn tui_field_rate(&self) -> &'static str {
        "帯域上限 (Mbit/s)"
    }

    fn tui_field_taier_browse(&self) -> &'static str {
        "拠点を選択（ツリー）"
    }

    fn tui_field_taier_targets(&self) -> &'static str {
        "ターゲットセレクタ"
    }

    fn tui_taier_browse_value(&self, selections: &str) -> String {
        let count = selections
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .count();
        if count == 0 {
            "[Enter] 省・都市・キャリアを選択（既定：北京・上海・広州）".to_owned()
        } else {
            format!("[Enter] 省・都市・キャリアを選択（{count} 件指定中）")
        }
    }

    fn tui_taier_picker_title(&self) -> &'static str {
        "中国キャリア拠点セレクタ (network.taier)"
    }

    fn tui_taier_picker_root(&self) -> &'static str {
        "すべての地域"
    }

    fn tui_taier_picker_loading(&self) -> &'static str {
        "CAICT GlobalSpeed 拠点リストを取得中...（Esc でキャンセル）"
    }

    fn tui_taier_picker_error_hint(&self) -> &'static str {
        "Enter / r：再試行  |  Esc：閉じる"
    }

    fn tui_taier_picker_tip(&self) -> &'static str {
        "直轄市は直接キャリアへ、省は都市一覧へ進みます。/ キーで絞り込み検索。"
    }

    fn tui_taier_picker_keys(&self) -> &'static str {
        "Space:切替  Enter/右:展開  左:戻る  /:検索  a:全選択  d:既定  c:解除  s/Esc:適用  q:取消"
    }

    fn tui_taier_operator_label(&self, code: &str) -> &'static str {
        match code {
            "telecom" => "中国電信 Telecom (ct)",
            "unicom" => "中国聯通 Unicom (cu)",
            "mobile" => "中国移動 Mobile (cm)",
            "cernet" => "教育網 CERNET (edu)",
            "cbn" => "中国広電 Broadnet (cbn)",
            "other" => "その他・海外 (other)",
            _ => "キャリア",
        }
    }

    fn tui_taier_picker_empty_err(&self) -> String {
        "少なくとも 1 つのキャリア拠点を選択してください（d で既定値、q でキャンセル）".to_owned()
    }

    fn tui_field_cloudflare_concurrency(&self) -> &'static str {
        "並列ワーカー数"
    }

    fn tui_field_cloudflare_udp(&self) -> &'static str {
        "STUN UDP ロス調査"
    }

    fn tui_field_inetspeed_threads(&self) -> &'static str {
        "マルチスレッド数"
    }

    fn tui_field_inetspeed_timeout(&self) -> &'static str {
        "スレッド制限時間 (秒)"
    }

    fn tui_field_inetspeed_max(&self) -> &'static str {
        "スレッド転送上限"
    }

    fn tui_field_inetspeed_latency_count(&self) -> &'static str {
        "無負荷遅延プローブ数"
    }

    fn tui_field_inetspeed_endpoint(&self) -> &'static str {
        "固定エッジ IP"
    }

    fn tui_field_trace_group(&self, group: &str) -> String {
        format!("グループ：{group}")
    }

    fn tui_field_targets(&self) -> &'static str {
        "ターゲット ID 指定"
    }

    fn tui_field_geo(&self) -> &'static str {
        "ASN・位置情報の照会"
    }

    fn tui_field_trace_tcp(&self) -> &'static str {
        "中国向け TCP 大容量追跡"
    }

    fn tui_field_trace_packet_size(&self) -> &'static str {
        "TCP パケット長 (バイト)"
    }

    fn tui_field_bgp_upload(&self) -> &'static str {
        "トポロジー画像の公開"
    }

    fn tui_field_dnsbl(&self) -> &'static str {
        "DNSBL 照会 (25 カ所)"
    }

    fn tui_field_mail(&self) -> &'static str {
        "25 番メールポート確認"
    }

    fn tui_field_media_group(&self, group: &str) -> String {
        format!("地域：{group}")
    }

    fn tui_field_services(&self) -> &'static str {
        "サービス ID 指定"
    }

    fn tui_field_language(&self) -> &'static str {
        "表示・レポート言語"
    }

    fn tui_lang_picker_title(&self) -> &'static str {
        "表示・レポート言語の選択"
    }

    fn tui_lang_picker_keys(&self) -> &'static str {
        "上下: 移動  Enter/Space: 選択  Esc/q: キャンセル"
    }

    fn tui_field_format(&self) -> &'static str {
        "レポート形式"
    }

    fn tui_field_output(&self) -> &'static str {
        "ファイル保存先"
    }

    fn tui_field_work_dir(&self) -> &'static str {
        "作業ディレクトリ"
    }

    fn tui_field_show_ip(&self) -> &'static str {
        "グローバル IP 表示"
    }

    fn tui_field_connections(&self) -> &'static str {
        "ダウンロード接続数"
    }

    fn tui_field_socks5_proxy(&self) -> &'static str {
        "SOCKS5 プロキシ"
    }

    fn tui_field_github_proxy(&self) -> &'static str {
        "GitHub ミラー URL"
    }

    fn tui_field_verbose(&self) -> &'static str {
        "詳細デバッグログ"
    }

    fn tui_default_locations(&self) -> &'static str {
        "既定（主要 5 拠点）"
    }

    fn tui_default_taier_cities(&self) -> &'static str {
        "既定（北京・上海・広州）"
    }

    fn tui_default_inetspeed_endpoint(&self) -> &'static str {
        "自動（Dual DoH + 最小 RTT）"
    }

    fn tui_default_none(&self) -> &'static str {
        "なし"
    }

    fn tui_unlimited(&self) -> &'static str {
        "無制限"
    }

    fn tui_default_cpu_note(&self) -> &'static str {
        "y-cruncher と Prime95 は任意選択です（x86_64 専用、ARM64 では自動スキップ）。"
    }

    fn tui_default_taier_note(&self) -> &'static str {
        "既定で北京・上海・広州の電信・聯通・移動 3 キャリアを測定します。"
    }

    fn tui_default_cloudflare_note(&self) -> &'static str {
        "speed.cloudflare.com の HTTPS 速度、Bufferbloat、STUN UDP ロスを測定します。"
    }

    fn tui_default_inetspeed_note(&self) -> &'static str {
        "Dual DoH で最適な mensura.cdn-apple.com IP を選び 1T / NT 速度を測定します。"
    }

    fn tui_default_trace_note(&self) -> &'static str {
        "グループ未選択時は最寄りキャッシュと既定キャリア経路を追跡します。"
    }

    fn tui_default_bgp_note(&self) -> &'static str {
        "text モード時に匿名化した AS パス SVG を imageupload.app へアップロードします。"
    }

    fn tui_default_media_note(&self) -> &'static str {
        "地域未選択時は世界 5 地域のすべての配信サービスを判定します。"
    }

    fn tui_masked(&self) -> &'static str {
        "伏せ字（既定）"
    }

    fn tui_unmasked(&self) -> &'static str {
        "完全表示（フル IP）"
    }

    fn tui_edit_title(&self, field: &str) -> String {
        format!("{field} の編集")
    }

    fn tui_edit_hint(&self) -> &'static str {
        "Enter: 適用  |  Ctrl-U: クリア（既定に戻す）  |  Esc: キャンセル"
    }

    fn tui_no_items_selected(&self) -> String {
        "開始する前に少なくとも 1 つのテスト項目を有効にしてください".to_owned()
    }

    fn tui_no_cpu_suites_selected(&self) -> String {
        "cpu.bench のサブスイートを 1 つ以上選択してください（または cpu.bench を無効化）"
            .to_owned()
    }

    fn tui_invalid_number(&self, field: &str, min: u32, max: u32) -> String {
        format!("{field} には {min} 〜 {max} の整数を入力してください")
    }

    fn tui_empty_work_dir(&self) -> String {
        "作業ディレクトリは空にできません（カレントは . を指定）".to_owned()
    }

    fn tui_all_items_and_options(&self) -> &'static str {
        "全項目およびオプション"
    }

    fn tui_reset_done(&self, target: &str) -> String {
        format!("{target} を既定値にリセットしました")
    }

    fn tui_gauge_completed(&self, done: usize, total: usize) -> String {
        format!("{done} / {total} 完了")
    }

    fn tui_taier_picker_badge(&self, selected: usize, est_secs: u64) -> String {
        let est_text = if est_secs >= 60 {
            format!("~{}分 {:02}秒", est_secs / 60, est_secs % 60)
        } else {
            format!("~{est_secs}秒")
        };
        format!("{selected} 件選択 ({est_text})")
    }

    fn tui_taier_filter_prefix(&self) -> &'static str {
        "絞り込み："
    }

    fn tui_taier_filter_active(&self, filter: &str) -> String {
        format!("絞り込み中：\"{filter}\"（/: 編集、Esc: 解除）")
    }

    fn tui_taier_no_matches(&self) -> &'static str {
        "一致する拠点がありません（Esc で絞り込み解除）"
    }

    fn tui_taier_targets_preview(&self, targets: Option<&str>) -> String {
        match targets {
            Some(text) => format!("選択対象：{text}"),
            None => "選択対象：（未選択）".to_owned(),
        }
    }

    fn tui_taier_region_meta(&self, municipality: bool, overseas: bool, cities: usize) -> String {
        if municipality {
            "  （直轄市）".to_owned()
        } else if overseas {
            format!("  （海外 {cities} 拠点）")
        } else {
            format!("  （{cities} 都市）")
        }
    }

    fn tui_taier_node_count(&self, count: usize) -> String {
        if count == 1 {
            "1 ノード".to_owned()
        } else {
            format!("候補 {count} ノード")
        }
    }

    fn tui_keys_config_list(&self) -> &'static str {
        "上下:移動  Space:切替  右/Tab:設定  g:カテゴリ  a:全選択  d:初期化  L:言語  r:開始  q:終了"
    }

    fn tui_keys_config_params(&self) -> &'static str {
        "上下:項目  Space/Enter:切替/編集  左右:調整  Esc/Tab:戻る  d:初期化  L:言語  r:開始"
    }

    fn tui_progress_pane(&self, done: usize, total: usize) -> String {
        format!("進捗 ({done}/{total})")
    }

    fn tui_log_pane(&self, following: bool) -> &'static str {
        if following {
            "イベントログ（自動追従）"
        } else {
            "イベントログ（一時停止 - End で最新へ）"
        }
    }

    fn tui_collecting_host(&self) -> &'static str {
        "ホスト情報と IPv4/IPv6 疎通状況を確認しています..."
    }

    fn tui_state_queued(&self) -> &'static str {
        "待機中"
    }

    fn tui_state_running(&self) -> &'static str {
        "実行中"
    }

    fn tui_state_not_applicable(&self) -> &'static str {
        "この環境では対象外"
    }

    fn tui_stopping_banner(&self) -> &'static str {
        "途中レポート作成のため現在のテストを停止中... もう一度 Ctrl-C で即終了"
    }

    fn tui_keys_running(&self) -> &'static str {
        "上下/PgUp/PgDn: ログ移動  End: 最新を追従  Ctrl-C: 停止して途中レポートを表示"
    }

    fn tui_sections_pane(&self) -> &'static str {
        "セクション"
    }

    fn tui_section_top(&self) -> &'static str {
        "先頭 / 概要"
    }

    fn tui_report_pane(&self, format: &str, wrapped: bool, show_ip: bool) -> String {
        let wrap = if format == "text" {
            if wrapped {
                "、折返し:入"
            } else {
                "、折返し:切"
            }
        } else {
            ""
        };
        let ip = if show_ip { "フル IP" } else { "伏せ字 IP" };
        format!("レポート（{format}{wrap}、{ip}）")
    }

    fn tui_save_prompt_title(&self) -> &'static str {
        "レポートをファイルに保存"
    }

    fn tui_saved_to(&self, path: &Path) -> String {
        format!("レポートを {} に保存しました", path.display())
    }

    fn tui_keys_results(&self) -> &'static str {
        "矢印/PgUp/PgDn:移動  [/]:節  b:目次  f:形式  w:折返し  i:IP  L:言語  s:保存  \
         p:出力終了  n:新規  q:終了"
    }
}
