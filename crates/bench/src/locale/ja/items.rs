//! 日本語：各テスト項目の詳細セクション文言。

use std::path::Path;

use super::Japanese;
use crate::locale::BgpField;
use crate::locale::BgpIssueRef;
use crate::locale::BgpWords;
use crate::locale::CloudflareChoice;
use crate::locale::CloudflareField;
use crate::locale::CloudflareIssueRef;
use crate::locale::CloudflareWords;
use crate::locale::CpuWords;
use crate::locale::DiskWords;
use crate::locale::FioRow;
use crate::locale::InetspeedChoice;
use crate::locale::InetspeedProbeFaultRef;
use crate::locale::InetspeedRoundIssueRef;
use crate::locale::InetspeedWarningRef;
use crate::locale::InetspeedWords;
use crate::locale::IpChoice;
use crate::locale::IpField;
use crate::locale::IpTable;
use crate::locale::IpWords;
use crate::locale::ListRule;
use crate::locale::LookupProblem;
use crate::locale::MediaCell;
use crate::locale::MediaChoice;
use crate::locale::MediaWords;
use crate::locale::MemoryWords;
use crate::locale::NetworkWords;
use crate::locale::NoSwapWhy;
use crate::locale::Probe;
use crate::locale::TaierCell;
use crate::locale::TaierChoice;
use crate::locale::TaierListProblem;
use crate::locale::TaierRule;
use crate::locale::TaierSelectorProblem;
use crate::locale::TaierWords;
use crate::locale::TraceChoice;
use crate::locale::TraceList;
use crate::locale::TraceListProblem;
use crate::locale::TraceRule;
use crate::locale::TraceWords;
use crate::locale::Verdict;
use crate::locale::WayCell;

fn version(version: Option<&str>) -> String {
    version.map_or_else(String::new, |version| format!("{version}："))
}

impl CpuWords for Japanese {
    fn cpu_suite_step(&self, suite: &str, stage: Option<&str>, threads: Option<usize>) -> String {
        let name = self.cpu_suite_title(suite);
        match (stage, threads) {
            (Some(stage), Some(1)) => format!("{name}：{stage}（1 並列）"),
            (Some(stage), Some(n)) => format!("{name}：{stage}（{n} スレッド/並列）"),
            (Some(stage), None) => format!("{name}：{stage}"),
            (None, Some(1)) => format!("{name}：シングルスレッド測定"),
            (None, Some(n)) => format!("{name}：マルチスレッド測定（{n} スレッド）"),
            (None, None) => format!("{name}：実行中"),
        }
    }

    fn cpu_selected_suites(&self, suites: &[String]) -> String {
        format!("選択スイート：{}", suites.join(", "))
    }

    fn cpu_suite_title(&self, suite: &str) -> &'static str {
        match suite {
            "sysbench" => "sysbench",
            "7zip" => "7-Zip",
            "unixbench" => "UnixBench",
            "hpl" => "HPL (Linpack)",
            "passmark" => "PassMark",
            "ycruncher" => "y-cruncher",
            "prime95" => "Prime95",
            "geekbench" => "Geekbench 7",
            _ => "CPU スイート",
        }
    }

    fn cpu_suite_skipped(&self, suite: &str, why: &str) -> String {
        format!("{}：スキップ（{why}）", self.cpu_suite_title(suite))
    }

    fn cpu_suite_failed(&self, suite: &str, why: &str) -> String {
        format!("{}：失敗（{why}）", self.cpu_suite_title(suite))
    }

    fn cpu_skip_arch(&self, arch: &str) -> String {
        format!("x86_64 専用；このホストは {arch}")
    }

    fn cpu_missing_lib(&self, lib: &str) -> String {
        format!("ホスト共有ライブラリ {lib} が不足しています")
    }

    fn cpu_setup_failed(&self, reason: &str) -> String {
        format!("作業ディレクトリを準備できません：{reason}")
    }

    fn sysbench_parameters(&self, version: Option<&str>, max_prime: u64, seconds: u64) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        format!("sysbench{version}：素数上限 {max_prime}、各 {seconds} 秒")
    }

    fn cpu_headers(&self) -> [&'static str; 3] {
        ["テスト", "スレッド数", "イベント/秒"]
    }

    fn cpu_row(&self, multi: bool) -> &'static str {
        if multi {
            "マルチスレッド"
        } else {
            "シングルスレッド"
        }
    }

    fn cpu_test(&self, multi: bool) -> &'static str {
        if multi {
            "マルチスレッド測定"
        } else {
            "シングルスレッド測定"
        }
    }

    fn ratio_label(&self) -> &'static str {
        "マルチ倍率"
    }

    fn ratio(&self, ratio: f64) -> String {
        format!("マルチスレッドはシングルスレッドの {ratio:.2} 倍")
    }

    fn no_rate(&self) -> String {
        "sysbench が秒間イベント数を出力しませんでした".to_owned()
    }

    fn seven_zip_parameters(&self, version: Option<&str>) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        format!("7-Zip{version}：LZMA2 整数圧縮・展開ベンチマーク")
    }

    fn seven_zip_headers(&self) -> [&'static str; 5] {
        [
            "テスト",
            "スレッド数",
            "圧縮 (MIPS)",
            "展開 (MIPS)",
            "合計 (MIPS)",
        ]
    }

    fn seven_zip_no_scores(&self) -> String {
        "7-Zip がベンチマーク評価値を出力しませんでした".to_owned()
    }

    fn unixbench_parameters(&self, version: &str) -> String {
        format!("Byte-UnixBench {version}：12 種システム総合指数（SPARCstation 20-61 = 10.0）")
    }

    fn unixbench_index_label(&self, copies: usize) -> String {
        if copies <= 1 {
            "総合指数（1 並列）".to_owned()
        } else {
            format!("総合指数（{copies} 並列）")
        }
    }

    fn unixbench_headers(&self, multi_copies: Option<usize>) -> Vec<String> {
        match multi_copies {
            Some(n) => vec![
                "項目".to_owned(),
                "1 並列".to_owned(),
                format!("{n} 並列"),
                "単位".to_owned(),
                "1-指数".to_owned(),
                format!("{n}-指数"),
            ],
            None => vec![
                "項目".to_owned(),
                "1 並列".to_owned(),
                "単位".to_owned(),
                "1-指数".to_owned(),
            ],
        }
    }

    fn unixbench_item_name(&self, id: &str) -> &'static str {
        match id {
            "dhry2reg" => "Dhrystone 2 整数演算",
            "whetstone-double" => "倍精度 Whetstone 浮動小数点",
            "execl" => "Execl スループット",
            "fstime" => "ファイルコピー (1024 B, 2000 blk)",
            "fsbuffer" => "ファイルコピー (256 B, 500 blk)",
            "fsdisk" => "ファイルコピー (4096 B, 8000 blk)",
            "pipe" => "パイプスループット",
            "context1" => "パイプコンテキストスイッチ",
            "spawn" => "プロセス生成",
            "shell1" => "シェルスクリプト (1 並列)",
            "shell8" => "シェルスクリプト (8 並列)",
            "syscall" => "システムコールオーバーヘッド",
            _ => "UnixBench 項目",
        }
    }

    fn unixbench_no_count(&self, item: &str) -> String {
        format!("UnixBench {item} が COUNT 値を出力しませんでした")
    }

    fn unixbench_timed_out(&self, item: &str, seconds: u64) -> String {
        format!("UnixBench {item} が {seconds} 秒経過しても終了しないため停止")
    }

    fn hpl_parameters(&self, version: Option<&str>, n: u32, nb: u32, threads: usize) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        format!("HPL{ver} (OpenBLAS)：倍精度密行列 LU 分解、N={n}、NB={nb}、{threads} スレッド")
    }

    fn hpl_labels(&self) -> [&'static str; 3] {
        ["演算性能", "計算時間", "残差検証"]
    }

    fn hpl_gflops(&self, gflops: f64) -> String {
        format!("{gflops:.2} GFLOPS")
    }

    fn hpl_seconds(&self, seconds: f64) -> String {
        format!("{seconds:.2} s")
    }

    fn hpl_residual_passed(&self, residual: Option<f64>) -> String {
        match residual {
            Some(res) => format!("合格 ({res:.2e})"),
            None => "合格".to_owned(),
        }
    }

    fn hpl_residual_failed(&self) -> String {
        "HPL スケール残差の数値検証に失敗しました".to_owned()
    }

    fn passmark_parameters(&self, version: Option<&str>) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        format!("PassMark PerformanceTest{ver}：Linux CPU スイート")
    }

    fn passmark_summary_labels(&self) -> [&'static str; 2] {
        ["CPU Mark", "シングルスレッド"]
    }

    fn passmark_headers(&self) -> [&'static str; 3] {
        ["サブテスト", "スコア", "単位"]
    }

    fn passmark_subtests(&self) -> [(&'static str, &'static str); 8] {
        [
            ("整数演算", "MOps/s"),
            ("浮動小数点演算", "MOps/s"),
            ("素数探索", "MPrimes/s"),
            ("文字列ソート", "KStrings/s"),
            ("暗号化処理", "MB/s"),
            ("データ圧縮", "KB/s"),
            ("物理演算", "Frames/s"),
            ("拡張命令セット", "MMatrices/s"),
        ]
    }

    fn passmark_no_scores(&self) -> String {
        "PassMark が CPU スコアを出力しませんでした".to_owned()
    }

    fn ycruncher_parameters(
        &self,
        version: Option<&str>,
        digits: u64,
        tuning: Option<&str>,
    ) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        let millions = digits / 1_000_000;
        let tune = tuning.map_or_else(String::new, |t| format!(" ({t})"));
        format!("y-cruncher{ver}：円周率 Pi {millions}M 桁計算{tune}")
    }

    fn ycruncher_labels(&self) -> [&'static str; 4] {
        ["純計算時間", "実経過時間", "CPU 使用率", "マルチコア効率"]
    }

    fn ycruncher_validation_failed(&self) -> String {
        "y-cruncher 円周率桁データの検算に失敗しました".to_owned()
    }

    fn prime95_parameters(
        &self,
        version: Option<&str>,
        workers: usize,
        duration_secs: u64,
        fft: Option<&str>,
    ) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        let kernel = fft.map_or_else(String::new, |f| format!("、{f}"));
        format!(
            "Prime95{ver}：Small-FFT 負荷テスト、{workers} ワーカー、{duration_secs} 秒{kernel}"
        )
    }

    fn prime95_labels(&self) -> [&'static str; 2] {
        ["安定性判定", "完了テスト数"]
    }

    fn prime95_passed(&self, warnings: u64) -> String {
        format!("合格（エラー 0、警告 {warnings}）")
    }

    fn prime95_hardware_error(&self, detail: &str) -> String {
        format!("Prime95 がハードウェア・丸め誤差エラーを検出：{detail}")
    }

    fn prime95_no_summary(&self) -> String {
        "Prime95 がワーカー集計を出力せずに終了しました".to_owned()
    }

    fn geekbench_labels(&self) -> [&'static str; 3] {
        ["シングルコア", "マルチコア", "結果 URL"]
    }

    fn swapped(&self, need: u64, available: &str, swap: u64) -> String {
        format!(
            "空きメモリ不足（約 {need} GiB 必要、空き {available} GiB）；\
             一時 Swap を {swap} GiB 追加したためスコアが低く出る場合があります"
        )
    }

    fn memory_short(&self, need: u64, available: &str, why: NoSwapWhy<'_>) -> String {
        let why = match why {
            NoSwapWhy::Unused => {
                "vm.swappiness が 0 のためカーネルがメモリをスワップしません".to_owned()
            }
            NoSwapWhy::NoRoom(Some(free)) => {
                format!("作業ディスクの空き容量が {free} GiB しかありません")
            }
            NoSwapWhy::NoRoom(None) => "作業ディスクの空き容量を読み取れません".to_owned(),
            NoSwapWhy::Denied => "権限なし（コンテナ環境では通常禁止）".to_owned(),
            NoSwapWhy::Unsupported => {
                "作業ディレクトリのファイルシステムが Swap 非対応です".to_owned()
            }
            NoSwapWhy::Error(error) => error.to_owned(),
        };
        format!(
            "空きメモリ不足の可能性（約 {need} GiB 必要、空き {available} GiB）；\
             一時 Swap なし：{why}"
        )
    }

    fn first_to_go_not_set(&self, error: &str) -> String {
        format!("Geekbench の OOM 優先終了設定に失敗：{error}")
    }

    fn oom_not_restored(&self, error: &str) -> String {
        format!("自身の oom_score_adj を復元できません：{error}")
    }

    fn uploads_retried(&self, count: usize, last: &str) -> String {
        format!("{count} 回のアップロード失敗後に再実行して成功；理由：{last}")
    }

    fn swap_left(&self, error: &str, path: &Path) -> String {
        format!(
            "一時 Swap を解除できません：{error}；手動で swapoff {} を実行し削除してください",
            path.display()
        )
    }

    fn upload_failed(&self, reason: &str) -> String {
        format!(
            "アップロード失敗（{reason}）；無料版 Geekbench は\
             アップロード後にのみスコアを表示します"
        )
    }

    fn no_scores(&self) -> String {
        "Geekbench がスコアを出力しませんでした".to_owned()
    }

    fn out_of_memory(&self) -> String {
        "実行中にメモリ不足（OOM）によりシステムから強制終了されました".to_owned()
    }

    fn cpu_no_suites_selected(&self) -> String {
        "CPU ベンチマークスイートが選択されていません".to_owned()
    }

    fn cpu_all_skipped(&self) -> String {
        "選択されたすべての CPU スイートがこのホストでは対象外のためスキップされました".to_owned()
    }

    fn cpu_all_failed(&self) -> String {
        "実行可能なすべての CPU ベンチマークスイートが失敗しました".to_owned()
    }
}

impl MemoryWords for Japanese {
    fn memory_sysbench(
        &self,
        version: Option<&str>,
        seconds: u64,
        single: &str,
        per_thread: Option<&str>,
    ) -> String {
        let mut text = format!(
            "{}シーケンシャル読み書き、各 {seconds} 秒；単一スレッド {single}",
            self::version(version)
        );
        if let Some(per_thread) = per_thread {
            text.push_str(&format!("、マルチ時は 1 スレッドあたり {per_thread}"));
        }
        text
    }

    fn memory_stream(
        &self,
        version: Option<&str>,
        millions: u64,
        array: u64,
        arrays: u64,
        spread: bool,
    ) -> String {
        let spread = if spread {
            "；スレッドを各ノードへ均等配置"
        } else {
            ""
        };
        format!(
            "{}1 配列あたり {millions}00 万要素（{array} MiB、3 配列計 {arrays} MiB）、\
             10 回試行の最速値{spread}",
            self::version(version)
        )
    }

    fn latency_label(&self) -> &'static str {
        "アクセス遅延"
    }

    fn latency_method(&self) -> String {
        "multichase：ランダムポインタチェイス、各階層 2.5 秒".to_owned()
    }

    fn numa_method(&self, nodes: usize) -> String {
        format!("{nodes} ノード；帯域幅は multiload stream-triad（3 回最速値）、遅延は同上")
    }

    fn results(&self, tool: &str) -> String {
        format!("{tool} 測定結果")
    }

    fn numa_title(&self, probe: Probe) -> &'static str {
        match probe {
            Probe::Bandwidth => "NUMA 帯域幅（MiB/s；行：スレッド側ノード、列：メモリ側ノード）",
            Probe::Latency => "NUMA アクセス遅延 (ns)",
        }
    }

    fn test_header(&self) -> &'static str {
        "項目"
    }

    fn threads_column(&self, threads: usize) -> String {
        if threads > 1 {
            format!("マルチスレッド（{threads} スレッド）")
        } else {
            "シングルスレッド".to_owned()
        }
    }

    fn latency_headers(&self) -> [&'static str; 3] {
        ["階層", "バッファサイズ", "遅延"]
    }

    fn node(&self, node: usize) -> String {
        format!("ノード {node}")
    }

    fn operation(&self, write: bool) -> &'static str {
        if write { "書込" } else { "読込" }
    }

    fn memory_level(&self) -> &'static str {
        "メインメモリ"
    }

    fn cache_unknown(&self) -> String {
        "最終レベルキャッシュ容量が不明なため 32 MiB と仮定".to_owned()
    }

    fn stream_rule(&self) -> String {
        "空きメモリの制約により STREAM 標準条件（各配列が LLC の 4 倍以上）未満です".to_owned()
    }

    fn sysbench_small(&self, four: &str) -> String {
        format!(
            "空きメモリ少：測定データが最終キャッシュの 4 倍（{four}）未満のため、\
             キャッシュヒットにより値が高めに出ます"
        )
    }

    fn latency_small(&self, four: &str) -> String {
        format!(
            "空きメモリ少：メモリ領域が最終キャッシュの 4 倍（{four}）未満のため、\
             キャッシュヒットにより遅延が低めに出ます"
        )
    }

    fn levels_missing(&self, levels: &[&str]) -> String {
        match levels {
            [level] => format!("{level} の容量情報がないため省略しました"),
            [others @ .., last] => format!(
                "{} および {last} の容量情報がないため省略しました",
                others.join("、")
            ),
            [] => String::new(),
        }
    }

    fn sysbench_run(&self, write: bool, multi: bool) -> String {
        let threads = if multi {
            "マルチスレッド"
        } else {
            "シングルスレッド"
        };
        let operation = if write { "書込" } else { "読込" };
        format!("{threads}{operation}")
    }

    fn stream_run(&self, multi: bool) -> String {
        if multi {
            "マルチスレッド測定".to_owned()
        } else {
            "シングルスレッド測定".to_owned()
        }
    }

    fn latency_run(&self, level: &str) -> String {
        format!("{level} 測定")
    }

    fn numa_run(&self, row: usize, column: usize, probe: Probe) -> String {
        let test = match probe {
            Probe::Bandwidth => "帯域幅",
            Probe::Latency => "遅延",
        };
        format!("ノード {row} スレッド・ノード {column} メモリの{test}測定")
    }

    fn no_transfer_rate(&self) -> String {
        "sysbench が転送速度を出力しませんでした".to_owned()
    }

    fn stream_invalid(&self, line: &str) -> String {
        format!("STREAM の検算チェックに失敗：{line}")
    }

    fn no_result(&self, tool: &str) -> String {
        format!("{tool} が測定結果を出力しませんでした")
    }

    fn no_latency(&self) -> String {
        "multichase が遅延値を出力しませんでした".to_owned()
    }

    fn no_bandwidth(&self) -> String {
        "multiload が帯域幅を出力しませんでした".to_owned()
    }

    fn bind_refused(&self) -> String {
        "コンテナの制限により NUMA ノードへのメモリバインドが拒否されました".to_owned()
    }

    fn no_memory_info(&self) -> String {
        "利用可能メモリ（/proc/meminfo）を読み取れません".to_owned()
    }

    fn stream_short(&self, needed: u64, budget: &str) -> String {
        format!(
            "空きメモリ不足：3 配列で最低 {needed} MiB 必要ですが、\
             利用可能メモリの半分は {budget} です"
        )
    }

    fn one_node(&self) -> String {
        "複数の NUMA ノードが必要です".to_owned()
    }
}

impl DiskWords for Japanese {
    fn health_headers(&self) -> [&'static str; 8] {
        [
            "デバイス",
            "健康状態",
            "通電時間",
            "温度",
            "摩耗率",
            "総書込量",
            "不良セクタ",
            "リンクエラー",
        ]
    }

    fn card_disk(&self, numbers: &str, host: Option<u32>) -> String {
        match host {
            Some(host) => format!("RAID カード host{host} ディスク {numbers}"),
            None => format!("RAID カード ディスク {numbers}"),
        }
    }

    fn verdict(&self, verdict: Verdict) -> &'static str {
        match verdict {
            Verdict::Passed => "正常",
            Verdict::Failed => "異常",
            Verdict::Unknown => "不明",
            Verdict::Unsupported => "非対応",
            Verdict::ReadFailed => "読取失敗",
        }
    }

    fn hours(&self, hours: u64) -> String {
        format!("{hours} 時間")
    }

    fn disk_read_failed(&self, disk: &str, reason: &str) -> String {
        format!("{disk}：読み取り失敗：{reason}")
    }

    fn no_smart(&self, disk: &str) -> String {
        format!("{disk} は SMART 非対応です")
    }

    fn smart_off(&self, disk: &str) -> String {
        format!("{disk} は SMART が無効です（本ツールでは有効化しません）")
    }

    fn disk_read_timed_out(&self, seconds: u64) -> String {
        format!("{seconds} 秒経過しても読み取りが終わらないため停止")
    }

    fn unparsable(&self) -> String {
        "smartctl の出力を解析できません".to_owned()
    }

    fn unopened(&self) -> String {
        "デバイスを開けないか識別情報を読み取れません".to_owned()
    }

    fn arguments_refused(&self) -> String {
        "smartctl が引数を拒否しました".to_owned()
    }

    fn node_left(&self, path: &Path, error: &str) -> String {
        format!("{} を削除できません：{error}", path.display())
    }

    fn no_disks(&self) -> String {
        "smartctl がディスクを検出できませんでした".to_owned()
    }

    fn scan_timed_out(&self, seconds: u64) -> String {
        format!("smartctl のディスク走査が {seconds} 秒でタイムアウトしました")
    }

    fn scan_failed(&self, reason: &str) -> String {
        format!("smartctl のディスク走査に失敗：{reason}")
    }

    fn all_failed(&self) -> String {
        "いずれのディスクも読み取れませんでした".to_owned()
    }

    fn fio_method(&self, size: &str) -> (&'static str, String) {
        (
            "測定条件",
            format!("YABS 準拠：ランダム読込 50% / 書込 50%、テストファイル {size}"),
        )
    }

    fn zfs_note(&self, needed: u64) -> String {
        format!(
            "測定ディレクトリが ZFS 上にあり空き容量が {needed} GiB 未満のため、\
             スコアが正確でない可能性があります（YABS と同様の注意）"
        )
    }

    fn block_sizes(&self, first: &str, second: &str) -> String {
        format!("{first} および {second}")
    }

    fn block_size_header(&self) -> &'static str {
        "ブロックサイズ"
    }

    fn fio_row(&self, row: FioRow) -> &'static str {
        match row {
            FioRow::Read => "読込",
            FioRow::Write => "書込",
            FioRow::Total => "合計",
        }
    }

    fn rate(&self, speed: &str, iops: &str) -> String {
        format!("{speed} ({iops})")
    }

    fn fio_timed_out(&self) -> &'static str {
        "タイムアウト"
    }

    fn fio_error(&self) -> &'static str {
        "エラー"
    }

    fn block_timed_out(&self, block_size: &str, seconds: u64) -> String {
        format!("{block_size} が {seconds} 秒経過しても終了しないため停止")
    }

    fn block_failed(&self, block_size: &str, reason: &str) -> String {
        format!("{block_size} 失敗：{reason}")
    }

    fn no_space_info(&self) -> String {
        "測定ディレクトリの空き容量を読み取れません".to_owned()
    }

    fn in_memory(&self) -> String {
        "測定ディレクトリが RAM ディスク上にありメモリ測定になってしまいます；\
         ディスク上で実行するか --work-dir で指定してください"
            .to_owned()
    }

    fn no_space(&self, needed: &str, available: &str) -> String {
        format!("測定ディレクトリの空き容量が {needed} 未満です（残り {available}）")
    }

    fn no_direct_io(&self) -> String {
        "測定ディレクトリのファイルシステムがダイレクト I/O 非対応です".to_owned()
    }

    fn setup_failed(&self, reason: &str) -> String {
        format!("テストファイルを作成できません：{reason}")
    }

    fn no_block_size(&self) -> String {
        "4 種のブロックサイズすべてで測定結果を取得できませんでした".to_owned()
    }

    fn no_output(&self, tool: &str) -> String {
        format!("{tool} の出力がありません")
    }

    fn fio_reported(&self, error: &str) -> String {
        format!("fio がエラーコード {error} を報告")
    }

    fn fio_field(&self, field: usize) -> String {
        format!("fio 出力の第 {field} フィールドを解析できません")
    }
}

impl NetworkWords for Japanese {
    fn iperf3_method(
        &self,
        version: Option<&str>,
        streams: u8,
        seconds: u16,
        rate: Option<u32>,
    ) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        let rate = rate.map_or_else(String::new, |rate| {
            format!("、1 ストリーム上限 {rate} Mbit/s")
        });
        format!("iperf3{version}：{streams} 並列ストリーム、片道 {seconds} 秒、TCP{rate}")
    }

    fn family_title(&self, ipv6: bool) -> &'static str {
        if ipv6 { "IPv6" } else { "IPv4" }
    }

    fn network_headers(&self) -> [&'static str; 5] {
        ["拠点", "事業者", "上り速度", "下り速度", "遅延"]
    }

    fn place(&self, location: &str, country: &str) -> String {
        format!("{location}, {country}")
    }

    fn way_cell(&self, cell: WayCell) -> &'static str {
        match cell {
            WayCell::Busy => "混雑",
            WayCell::Unreachable => "到達不可",
            WayCell::TimedOut => "タイムアウト",
            WayCell::Error => "エラー",
            WayCell::NotTried => "未試行",
        }
    }

    fn tries(&self, place: &str, download: bool, tries: u32) -> String {
        let direction = if download { "下り" } else { "上り" };
        format!("{place}：{direction}測定に {tries} 回試行しました")
    }

    fn way_failed(&self, place: &str, download: bool, reason: &str) -> String {
        let direction = if download { "下り" } else { "上り" };
        format!("{place}：{direction}測定に失敗：{reason}")
    }

    fn nothing_measured(&self) -> String {
        "iperf3 の有効な測定値が得られませんでした".to_owned()
    }

    fn unknown_servers(&self, names: &[String]) -> String {
        format!("サーバー一覧に該当拠点がありません：{}", names.join(", "))
    }

    fn list_unreadable(&self, reason: &str) -> String {
        format!("サーバー一覧を読み取れません：{reason}")
    }

    fn list_too_large(&self, limit: usize) -> String {
        format!("サーバー一覧が上限 {limit} バイトを超過しています")
    }

    fn list_format(&self, reason: &str) -> String {
        format!("サーバー一覧の形式が不正です：{reason}")
    }

    fn list_newer(&self, version: u64) -> String {
        format!("サーバー一覧にはより新しい lowendaff-bench が必要です（形式 {version}）")
    }

    fn list_version(&self, version: u64) -> String {
        format!("サーバー一覧が存在しない形式バージョンを指定しています：{version}")
    }

    fn list_too_many(&self, count: usize, most: usize) -> String {
        format!("サーバー一覧の件数 {count} が上限 {most} を超えています")
    }

    fn list_rule(&self, position: usize, rule: ListRule) -> String {
        let rule = match rule {
            ListRule::Id => "id は英小文字・数字・ハイフンの 1〜40 文字",
            ListRule::Country => "country は英大文字 2 文字の国コード",
            ListRule::Host => "host はドメイン名または IP アドレス",
            ListRule::Ports => "ports は 1024..=65535 の単一または最大 64 個のポート範囲",
            ListRule::Families => "families は ipv4、ipv6、または both",
            ListRule::Text => "location・provider・link は印字可能な 1〜40 文字",
        };
        format!("サーバー一覧の第 {position} 項目が規約違反：{rule}")
    }

    fn list_duplicate(&self, id: &str) -> String {
        format!("サーバー一覧に重複した ID があります：{id}")
    }

    fn no_server(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "測定対象サーバーが選択されていません".to_owned()
        } else {
            format!(
                "サーバー一覧に {} の該当拠点がありません",
                unknown.join(", ")
            )
        }
    }

    fn no_family(&self) -> String {
        "選択されたサーバーはこのホストで利用可能な IP ファミリに対応していません".to_owned()
    }

    fn no_speed(&self) -> String {
        "いずれのサーバーからも速度を取得できませんでした".to_owned()
    }
}

impl TraceWords for Japanese {
    fn trace_method(&self, version: &str, choice: &TraceChoice) -> String {
        let mut method = format!("NextTrace {version}：ICMP、各ホップ 3 回送信、最大 30 ホップ");
        if choice.tcp {
            method.push_str(&format!(
                "；中国本土宛て {} バイト TCP パケット追跡を併用",
                choice.large_packets
            ));
        }
        if !choice.groups.is_empty() {
            method.push_str(&format!("；グループ {}", choice.groups.join(", ")));
        }
        if !choice.targets.is_empty() {
            method.push_str(&format!("；ターゲット {}", choice.targets.join(", ")));
        }
        if choice.list {
            method.push_str("；カスタムターゲット一覧");
        }
        if !choice.geo {
            method.push_str("；ASN・地域照会なし");
        }
        method
    }

    fn trace_heading(&self, name: &str, host: Option<&str>, outcome: &str) -> String {
        match host {
            Some(host) => format!("{name} ({host})：{outcome}"),
            None => format!("{name}：{outcome}"),
        }
    }

    fn trace_tcp_heading(&self, port: u16, packet_size: u16, outcome: &str) -> String {
        format!("TCP ポート {port}（{packet_size} バイトパケット）：{outcome}")
    }

    fn youtube(&self) -> &'static str {
        "YouTube"
    }

    fn netflix(&self, place: Option<(&str, &str)>) -> String {
        match place {
            Some((city, country)) => format!("Netflix, {city}, {country}"),
            None => "Netflix".to_owned(),
        }
    }

    fn reached(&self, hops: usize) -> String {
        format!("{hops} ホップで到達")
    }

    fn not_reached(&self, hops: usize) -> String {
        format!("{hops} ホップ以内に未到達")
    }

    fn fragmented(&self, hop: usize, mtu: Option<u16>) -> String {
        match mtu {
            Some(mtu) => format!("第 {hop} ホップで停止（次ホップの最大転送単位は {mtu} バイト）"),
            None => format!("第 {hop} ホップで停止（断片化なしでは通過不可）"),
        }
    }

    fn trace_headers(&self) -> [&'static str; 6] {
        ["Hop", "アドレス", "ASN", "ネットワーク", "地域", "遅延"]
    }

    fn trace_group_title(&self, code: &str) -> &'static str {
        match code {
            "services" => "主要サービスキャッシュ",
            "china" => "中国本土キャリア",
            "international" => "国際バックボーン・クラウド",
            _ => "経路追跡",
        }
    }

    fn trace_md_target_title(&self, name: &str, ipv6: bool) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{name} ({fam})")
    }

    fn trace_md_target_line(&self, host: Option<&str>, outcome: &str) -> String {
        match host {
            Some(host) => format!("宛先：{host}（{outcome}）"),
            None => outcome.to_owned(),
        }
    }

    fn at_exchange(&self, network: Option<&str>, exchange: &str) -> String {
        match network {
            Some(network) => format!("{network} [{exchange}]"),
            None => format!("[{exchange}]"),
        }
    }

    fn unavailable_lookup(&self, reason: &str) -> String {
        format!("取得不可：{reason}")
    }

    fn lookup_problem(&self, problem: LookupProblem) -> String {
        match problem {
            LookupProblem::NoServer => "Google の応答にサーバーが含まれていません",
            LookupProblem::Id => "Google 応答のサーバー ID をホスト名に変換できません",
            LookupProblem::Format => "fast.com の応答が API 形式ではありません",
            LookupProblem::NoTarget => "fast.com の応答にサーバーが含まれていません",
            LookupProblem::Url => "fast.com 応答のサーバーアドレスを追跡できません",
            LookupProblem::Place => "fast.com 応答の拠点名を表示できません",
        }
        .to_owned()
    }

    fn answer_unreadable(&self, reason: &str) -> String {
        format!("応答を読み取れません：{reason}")
    }

    fn traced_nothing(&self) -> String {
        "NextTrace の追跡結果がありません".to_owned()
    }

    fn unknown_targets(&self, ids: &[String]) -> String {
        format!("ターゲット一覧に該当がありません：{}", ids.join(", "))
    }

    fn no_exchanges(&self, reason: &str) -> String {
        format!("IX 一覧を利用できません：{reason}")
    }

    fn trace_list_problem(&self, list: TraceList, problem: TraceListProblem<'_>) -> String {
        let (name, entry) = match list {
            TraceList::Targets => ("ターゲット一覧", "ターゲット"),
            TraceList::Exchanges => ("IX ポート一覧", "ポート"),
        };
        match problem {
            TraceListProblem::Unreadable(reason) => format!("{name}を読み取れません：{reason}"),
            TraceListProblem::TooLarge(limit) => format!("{name}が上限 {limit} バイトを超過"),
            TraceListProblem::Format(reason) => format!("{name}の形式が不正：{reason}"),
            TraceListProblem::Newer(version) => {
                format!("{name}には新しい lowendaff-bench が必要です（形式 {version}）")
            }
            TraceListProblem::Version(version) => {
                format!("{name}が存在しない形式バージョンを指定：{version}")
            }
            TraceListProblem::TooMany(count, most) => {
                format!("{name}の{entry}数 {count} が上限 {most} を超過")
            }
            TraceListProblem::Rule(position, rule) => {
                let rule = match rule {
                    TraceRule::Id => {
                        "id は youtube / netflix 以外の英小文字・数字・- の 1〜40 文字"
                    }
                    TraceRule::Group => "group は china または international",
                    TraceRule::Name => "name は印字可能な 1〜40 文字",
                    TraceRule::Asn => "ASN は 1..=4294967295 の範囲",
                    TraceRule::Ipv4 => "ipv4 はドメイン名または IPv4 アドレス",
                    TraceRule::Ipv6 => "ipv6 はドメイン名または IPv6 アドレス",
                    TraceRule::Tcp => "tcp ポートは china ターゲットのみ指定可（1..=65535）",
                    TraceRule::Address => "address は正規化された IPv4 または純粋な IPv6",
                    TraceRule::Exchange => "exchange は印字可能な 1〜60 文字",
                };
                format!("{name}の第 {position} {entry}が規約違反：{rule}")
            }
            TraceListProblem::Duplicate(key) => match list {
                TraceList::Targets => format!("{name}に重複した ID があります：{key}"),
                TraceList::Exchanges => format!("{name}に重複したアドレスがあります：{key}"),
            },
        }
    }

    fn no_target(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "追跡ターゲットが選択されていません".to_owned()
        } else {
            format!("ターゲット一覧に {} がありません", unknown.join(", "))
        }
    }

    fn no_target_family(&self) -> String {
        "選択されたターゲットはこのホストの IP ファミリに対応していません".to_owned()
    }

    fn no_target_route(&self) -> String {
        "いずれのターゲットでも経路を取得できませんでした".to_owned()
    }
}

impl MediaWords for Japanese {
    fn media_method(&self, choice: &MediaChoice) -> String {
        let mut method = if choice.custom_base {
            "カスタム URL による直接プローブ（プロキシ不使用）".to_owned()
        } else {
            "HTTPS 直接プローブ（プロキシ不使用）".to_owned()
        };
        if !choice.groups.is_empty() {
            method.push_str(&format!("；地域 {}", choice.groups.join(", ")));
        }
        if !choice.services.is_empty() {
            method.push_str(&format!("；サービス {}", choice.services.join(", ")));
        }
        method
    }

    fn media_group(&self, code: &str) -> &'static str {
        match code {
            "global" => "グローバル",
            "asia" => "アジア",
            "europe" => "欧州",
            "americas" => "北米・中南米",
            "oceania" => "オセアニア",
            _ => "その他",
        }
    }

    fn media_headers(&self) -> [&'static str; 3] {
        ["サービス", "IPv4", "IPv6"]
    }

    fn media_yes(&self, region: Option<&str>, unlock: Option<&str>) -> String {
        let mode = match unlock {
            Some("native") => Some("ネイティブ"),
            Some("dns") => Some("DNS"),
            _ => None,
        };
        match (region, mode) {
            (Some(reg), Some(how)) => format!("視聴可 ({reg}, {how})"),
            (Some(reg), None) => format!("視聴可 ({reg})"),
            (None, Some(how)) => format!("視聴可 ({how})"),
            (None, None) => "視聴可".to_owned(),
        }
    }

    fn media_originals(&self, region: Option<&str>, unlock: Option<&str>) -> String {
        let mode = match unlock {
            Some("native") => Some("ネイティブ"),
            Some("dns") => Some("DNS"),
            _ => None,
        };
        match (region, mode) {
            (Some(reg), Some(how)) => format!("オリジナルのみ ({reg}, {how})"),
            (Some(reg), None) => format!("オリジナルのみ ({reg})"),
            (None, Some(how)) => format!("オリジナルのみ ({how})"),
            (None, None) => "オリジナルのみ".to_owned(),
        }
    }

    fn media_cell(&self, cell: MediaCell, ipv6: bool) -> &'static str {
        match cell {
            MediaCell::No => "不可",
            MediaCell::NoChina => "不可 (CN)",
            MediaCell::ComingSoon => "提供予定",
            MediaCell::ProxyDetected => "不可 (プロキシ検出)",
            MediaCell::BlockedWaf => "ブロック (WAF)",
            MediaCell::RateLimited => "レート制限",
            MediaCell::NoAddress => {
                if ipv6 {
                    "IPv6 アドレスなし"
                } else {
                    "IPv4 アドレスなし"
                }
            }
            MediaCell::TimedOut => "タイムアウト",
            MediaCell::Error => "エラー",
        }
    }

    fn media_unexpected(&self, service: &str, ipv6: bool, reason: &str) -> String {
        let family = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{service} ({family})：{reason}")
    }

    fn unknown_services(&self, ids: &[String]) -> String {
        format!("配信サービス一覧に該当がありません：{}", ids.join(", "))
    }

    fn invalid_base_url(&self, url: &str) -> String {
        format!(
            "base_url は https:// またはループバック \
             http://127.0.0.1 / http://[::1] で始めてください：{url}"
        )
    }

    fn no_service(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "配信サービスが選択されていません".to_owned()
        } else {
            format!("配信サービス一覧に {} がありません", unknown.join(", "))
        }
    }

    fn no_media_family(&self) -> String {
        "IPv4・IPv6 いずれの外部接続も利用できません".to_owned()
    }
}

impl IpWords for Japanese {
    fn ip_method(&self, choice: &IpChoice) -> String {
        let mut parts = vec!["9 つの評価ソース"];
        if choice.dnsbl {
            parts.push("DNSBL");
        }
        if choice.mail {
            parts.push("25 番メールポート");
        }
        let mut method = format!("直接照会（プロキシ不使用）：{}", parts.join("、"));
        if choice.custom_endpoints {
            method.push_str("；カスタムエンドポイント");
        }
        method
    }

    fn ip_stage(&self, stage: &str) -> &'static str {
        match stage {
            "intelligence" => "IP 評価ソース",
            "dnsbl" => "DNSBL ブラックリスト",
            "mail" => "25 番メールポート疎通",
            _ => "IP 品質",
        }
    }

    fn ip_table(&self, table: IpTable) -> &'static str {
        match table {
            IpTable::Summary => "基本情報",
            IpTable::Scores => "リスクスコア",
            IpTable::Factors => "リスク要因",
            IpTable::BlacklistsAndMail => "ブラックリスト・メール (25 番)",
        }
    }

    fn ip_headers(&self, table: IpTable) -> [&'static str; 3] {
        let first = match table {
            IpTable::Summary => "項目",
            IpTable::Scores => "評価ソース",
            IpTable::Factors => "リスク要素",
            IpTable::BlacklistsAndMail => "確認項目",
        };
        [first, "IPv4", "IPv6"]
    }

    fn ip_field(&self, field: IpField) -> &'static str {
        match field {
            IpField::Address => "IP アドレス",
            IpField::Hostname => "逆引きホスト名",
            IpField::Asn => "ASN",
            IpField::Organization => "組織名",
            IpField::Registry => "レジストリ",
            IpField::Origin => "IP 種別",
            IpField::Location => "都市・地域",
            IpField::Timezone => "タイムゾーン",
            IpField::AsnType => "ASN 区分",
            IpField::CompanyType => "組織区分",
            IpField::GeoIp => "所在地 (GeoIP)",
        }
    }

    fn ip_native(&self, country: &str) -> String {
        format!("ネイティブ IP ({country})")
    }

    fn ip_broadcast(&self, registered: &str, located: &str) -> String {
        format!("ブロードキャスト IP ({registered} -> {located})")
    }

    fn ip_usage(&self, code: &str) -> &'static str {
        match code {
            "residential" => "家庭用回線 (ISP)",
            "hosting" => "データセンター",
            "business" => "法人回線",
            "mobile" => "モバイル回線",
            "education" => "学術機関",
            "government" => "政府機関",
            "cdn" => "CDN",
            _ => "不明",
        }
    }

    fn ip_dual_isp(&self, usage: &str) -> String {
        format!("{usage}（デュアル ISP）")
    }

    fn ip_risk_level(&self, code: &str) -> &'static str {
        match code {
            "low" => "低リスク",
            "medium" => "中リスク",
            "high" => "高リスク",
            "very_high" => "非常に高い",
            _ => "不明",
        }
    }

    fn ip_numeric_score(&self, score: u8, max: u8, level: &str) -> String {
        format!("{score}/{max} ({})", self.ip_risk_level(level))
    }

    fn ip_ratio_score(&self, ratio: &str, level: &str) -> String {
        format!("{ratio} ({})", self.ip_risk_level(level))
    }

    fn ip_greynoise(&self, riot: bool, noise: bool, class: &str) -> &'static str {
        if riot {
            return "既知の良性サービス (RIOT)";
        }
        match class {
            "not_observed" if !noise => "クリーン（スキャン観測なし）",
            "benign" => "良性スキャナ",
            "malicious" => "悪性スキャナ",
            _ if noise => "スキャン活動を観測",
            _ => "クリーン（スキャン観測なし）",
        }
    }

    fn ip_shodan(&self, ports: usize, vulns: usize, flagged: bool) -> String {
        let label = if flagged || vulns > 0 {
            "要注意"
        } else {
            "クリーン"
        };
        format!("{label}（公開ポート {ports}、CVE {vulns}）")
    }

    fn ip_factor_name(&self, code: &str) -> &'static str {
        match code {
            "proxy" => "プロキシ (Proxy)",
            "vpn" => "VPN",
            "tor" => "Tor",
            "datacenter" => "データセンター",
            "abuser" => "不正利用歴",
            "robot" => "ボット (Robot)",
            "bogon" => "予約アドレス (Bogon)",
            _ => "その他",
        }
    }

    fn ip_factor_cell(&self, checked: usize, flagged: &[&str]) -> String {
        if flagged.is_empty() {
            format!("なし (0/{checked})")
        } else {
            format!("該当 ({}/{checked}：{})", flagged.len(), flagged.join(", "))
        }
    }

    fn ip_dnsbl_label(&self) -> &'static str {
        "DNSBL"
    }

    fn ip_dnsbl_cell(&self, checked: usize, refused: usize, listed: &[&str]) -> String {
        if listed.is_empty() {
            if refused > 0 {
                format!("クリーン (0/{checked}；{refused} 件照会拒否)")
            } else {
                format!("クリーン (0/{checked})")
            }
        } else {
            format!(
                "登録あり ({}/{checked}：{})",
                listed.len(),
                listed.join(", ")
            )
        }
    }

    fn ip_port25_label(&self) -> &'static str {
        "ポート 25"
    }

    fn ip_port25_cell(&self, open: bool) -> &'static str {
        if open { "開放" } else { "遮断" }
    }

    fn ip_mail_yes(&self, code: u16) -> String {
        format!("接続可 ({code})")
    }

    fn ip_mail_rejected(&self, code: u16) -> String {
        format!("拒否 ({code})")
    }

    fn ip_mail_blocked(&self) -> &'static str {
        "遮断"
    }

    fn ip_unexpected(&self, provider: &str, ipv6: bool, reason: &str) -> String {
        let family = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{provider} ({family})：{reason}")
    }

    fn ip_no_public_address(&self) -> String {
        "到達可能なネットワークファミリのグローバル IP が見つかりません".to_owned()
    }

    fn ip_invalid_endpoint(&self, raw: &str) -> String {
        format!(
            "上書きエンドポイントは https:// またはループバック \
             (127.0.0.1 / [::1]) が必要です：{raw}"
        )
    }
}

impl BgpWords for Japanese {
    fn bgp_method(&self, custom_endpoints: bool, upload: bool) -> String {
        let mut line = "RIPEstat RIS 直接照会：BGP 広告状態・RPKI・AS パス".to_owned();
        if !upload {
            line.push_str("；トポロジー画像アップロードなし");
        }
        if custom_endpoints {
            line.push_str("；カスタムエンドポイント");
        }
        line
    }

    fn bgp_summary_title(&self) -> &'static str {
        "BGP サマリー"
    }

    fn bgp_summary_headers(&self) -> [&'static str; 3] {
        ["項目", "IPv4", "IPv6"]
    }

    fn bgp_field(&self, field: BgpField) -> &'static str {
        match field {
            BgpField::Address => "IP アドレス",
            BgpField::Prefix => "広告プレフィックス",
            BgpField::OriginAsn => "オリジン ASN",
            BgpField::Rpki => "RPKI 状態",
            BgpField::Visibility => "RIS 観測状況",
            BgpField::Upstreams => "直接の上流 AS",
        }
    }

    fn bgp_not_announced(&self) -> &'static str {
        "広告なし"
    }

    fn bgp_origin_cell(&self, asn: u32, name: Option<&str>, total_origins: usize) -> String {
        let mut cell = match name.filter(|s| !s.is_empty()) {
            Some(label) => format!("AS{asn} ({label})"),
            None => format!("AS{asn}"),
        };
        if total_origins > 1 {
            cell.push_str(&format!(" (+{} MOAS)", total_origins - 1));
        }
        cell
    }

    fn bgp_rpki_cell(&self, code: &str) -> &'static str {
        match code {
            "valid" => "有効 (Valid)",
            "invalid" => "無効 (Invalid)",
            "not_found" => "未登録 (ROA なし)",
            _ => "不明",
        }
    }

    fn bgp_visibility_cell(&self, peers: u32, distinct_paths: usize) -> String {
        format!("{peers} ピア観測、{distinct_paths} 種類の経路")
    }

    fn bgp_direct_peering(&self) -> &'static str {
        "コレクタ直接ピアリング"
    }

    fn bgp_topology_title(&self, ipv6: bool) -> &'static str {
        if ipv6 {
            "IPv6 上流トポロジー"
        } else {
            "IPv4 上流トポロジー"
        }
    }

    fn bgp_paths_title(&self, ipv6: bool) -> &'static str {
        if ipv6 {
            "IPv6 観測パス"
        } else {
            "IPv4 観測パス"
        }
    }

    fn bgp_paths_headers(&self) -> [&'static str; 3] {
        ["ピア数", "ホップ数", "AS_PATH"]
    }

    fn bgp_issue(&self, issue: BgpIssueRef<'_>) -> String {
        match issue {
            BgpIssueRef::NoAddress => {
                "API ホストに該当 IP ファミリのアドレスがありません".to_owned()
            }
            BgpIssueRef::TimedOut => "リクエストがタイムアウトしました".to_owned(),
            BgpIssueRef::TooManyRedirects => "HTTP リダイレクトが多すぎます".to_owned(),
            BgpIssueRef::InvalidRedirect(why) => format!("不正なリダイレクト：{why}"),
            BgpIssueRef::NotHttps(why) => format!("非 HTTPS エンドポイント：{why}"),
            BgpIssueRef::Transport(why) => why.to_owned(),
            BgpIssueRef::HttpStatus(status) => format!("bgp-state が HTTP {status} を返却"),
            BgpIssueRef::InvalidJson(why) => format!("不正な bgp-state JSON：{why}"),
            BgpIssueRef::ApiStatus(status) => format!("bgp-state ステータス `{status}`"),
            BgpIssueRef::MissingData => "bgp-state 応答に `data` がありません".to_owned(),
            BgpIssueRef::NoData => "BGP データが返されませんでした".to_owned(),
        }
    }

    fn bgp_upload_http_error(&self, status: u16, reason: &str) -> String {
        format!("HTTP {status}：{reason}")
    }

    fn bgp_upload_invalid_json(&self) -> String {
        "不正な JSON 応答".to_owned()
    }

    fn bgp_upload_rejected(&self) -> String {
        "アップロードが拒否されました".to_owned()
    }

    fn bgp_upload_missing_key(&self) -> String {
        "応答に有効な imageKey がありません".to_owned()
    }

    fn bgp_upload_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} トポロジー画像のアップロード失敗：{reason}")
    }

    fn bgp_family_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} BGP 照会失敗：{reason}")
    }

    fn bgp_query_failed(&self, reason: &str) -> String {
        format!("BGP 経路状態の照会失敗：{reason}")
    }
}

impl TaierWords for Japanese {
    fn taier_method(&self, choice: &TaierChoice) -> String {
        let mut method = format!(
            "CAICT GlobalSpeed (Taier)：{} 並列ストリーム、片道 {} 秒、HTTP/1.1",
            choice.streams, choice.seconds
        );
        if !choice.selections.is_empty() {
            const MAX_SHOWN: usize = 5;
            if choice.selections.len() <= MAX_SHOWN {
                method.push_str(&format!("；対象 {}", choice.selections.join(", ")));
            } else {
                let head = choice.selections[..MAX_SHOWN].join(", ");
                let rest = choice.selections.len() - MAX_SHOWN;
                method.push_str(&format!("；対象 {head}（ほか {rest} 件）"));
            }
        } else {
            if !choice.cities.is_empty() {
                method.push_str(&format!("；都市 {}", choice.cities.join(", ")));
            }
            if !choice.operators.is_empty() {
                method.push_str(&format!("；キャリア {}", choice.operators.join(", ")));
            }
        }
        if choice.custom_list {
            method.push_str("；カスタムサーバー一覧");
        }
        method
    }

    fn taier_unknown_cities(&self, names: &[String]) -> String {
        format!("Taier 一覧に該当拠点がありません：{}", names.join(", "))
    }

    fn taier_headers(&self) -> [&'static str; 4] {
        ["拠点", "キャリア", "下り速度", "上り速度"]
    }

    fn taier_place(&self, city: &str, province: &str) -> String {
        if city.eq_ignore_ascii_case(province) {
            city.to_owned()
        } else {
            format!("{city}, {province}")
        }
    }

    fn taier_operator(&self, code: &str) -> &'static str {
        match code {
            "telecom" => "中国電信",
            "unicom" => "中国聯通",
            "mobile" => "中国移動",
            "cernet" => "教育網",
            "cbn" => "中国広電",
            _ => "その他",
        }
    }

    fn taier_cell(&self, cell: TaierCell) -> &'static str {
        match cell {
            TaierCell::Busy => "混雑",
            TaierCell::Unreachable => "到達不可",
            TaierCell::AuthRejected => "認証拒否",
            TaierCell::TimedOut => "タイムアウト",
            TaierCell::Error => "エラー",
        }
    }

    fn taier_fallback(&self, place: &str, operator: &str, tried: u8) -> String {
        format!("{place}（{operator}）：混雑・到達不可のため第 {tried} 候補ノードで測定しました")
    }

    fn taier_node_failed(&self, place: &str, operator: &str, reason: &str) -> String {
        format!("{place}（{operator}）：失敗：{reason}")
    }

    fn taier_list_problem(&self, problem: TaierListProblem<'_>) -> String {
        match problem {
            TaierListProblem::Missing => "Taier サーバー一覧がありません".to_owned(),
            TaierListProblem::Unreadable(reason) => {
                format!("Taier サーバー一覧を読み取れません：{reason}")
            }
            TaierListProblem::TooLarge(limit) => {
                format!("Taier サーバー一覧が上限 {limit} バイトを超過しています")
            }
            TaierListProblem::Format(reason) => {
                format!("Taier サーバー一覧の形式が不正です：{reason}")
            }
            TaierListProblem::Newer(version) => {
                format!("Taier 一覧には新しい lowendaff-bench が必要です（形式 {version}）")
            }
            TaierListProblem::Version(version) => {
                format!("Taier 一覧が存在しない形式バージョンを指定しています：{version}")
            }
            TaierListProblem::Empty => "Taier サーバー一覧に拠点がありません".to_owned(),
            TaierListProblem::TooMany(count, most) => {
                format!("Taier 一覧の拠点数 {count} が上限 {most} を超えています")
            }
            TaierListProblem::Rule(position, rule) => {
                let detail = match rule {
                    TaierRule::Id => "id は all 以外の英小文字・数字・- の 1〜40 文字",
                    TaierRule::Province => "province は印字可能な ASCII 1〜40 文字",
                    TaierRule::City => "city は印字可能な ASCII 1〜40 文字",
                    TaierRule::Nodes => "nodes は 1〜32 件のノード要素",
                    TaierRule::Operator => {
                        "operator は telecom, unicom, mobile, cernet, cbn, other のいずれか"
                    }
                    TaierRule::Ip => "ip は正規化された IPv4 アドレス",
                    TaierRule::Port => "port は 1024..=65535 の範囲",
                    TaierRule::DuplicateEndpoint => "同一拠点内に重複する ip:port があります",
                };
                format!("Taier 一覧の第 {position} 拠点が規約違反：{detail}")
            }
            TaierListProblem::Duplicate(id) => {
                format!("Taier 一覧に重複した拠点 ID があります：{id}")
            }
        }
    }

    fn taier_selector_problem(&self, problem: TaierSelectorProblem<'_>) -> String {
        match problem {
            TaierSelectorProblem::Empty => "空の Taier セレクタです".to_owned(),
            TaierSelectorProblem::Format(raw) => format!(
                "無効な Taier セレクタ `{raw}`：<拠点>[:<キャリア>[+<キャリア>...]] \
                 形式で指定してください（例：beijing:cu や guangdong/shenzhen:ct+cm）"
            ),
            TaierSelectorProblem::EmptyPlace(raw) => {
                format!("無効な Taier セレクタ `{raw}`：拠点名がありません")
            }
            TaierSelectorProblem::EmptyOperators(raw) => format!(
                "無効な Taier セレクタ `{raw}`：`:` の後にキャリアコードが必要です\
                 （ct, cu, cm, edu, cbn, other, all）"
            ),
            TaierSelectorProblem::UnknownOperator { selector, operator } => format!(
                "Taier セレクタ `{selector}` に不明なキャリア `{operator}`：telecom/ct, \
                 unicom/cu, mobile/cm, cernet/edu, cbn, other, all から指定してください"
            ),
        }
    }

    fn taier_catalog_header(&self, regions: usize, locations: usize) -> String {
        format!("中国キャリア回線速度 (network.taier)：{regions} 地域、{locations} 拠点")
    }

    fn taier_catalog_carriers_legend(&self) -> &'static str {
        "キャリア略号：ct = 中国電信、cu = 中国聯通、cm = 中国移動、\
         edu = 教育網、cbn = 中国広電、other = その他"
    }

    fn taier_region_kind(&self, code: &str) -> &'static str {
        match code {
            "municipality" => "直轄市",
            "province" => "省・自治区",
            _ => "海外・その他",
        }
    }

    fn taier_catalog_overview_headers(&self) -> [&'static str; 5] {
        ["地域", "区分", "都市数", "キャリア", "--taier セレクタ例"]
    }

    fn taier_catalog_footer_hint(&self) -> &'static str {
        "--taier-list <地域>（例：--taier-list guangdong）でその地域内の都市一覧を表示します。"
    }

    fn taier_catalog_region_heading(&self, name: &str, municipality: bool, count: usize) -> String {
        if municipality {
            format!("{name}（直轄市、{count} 拠点）：")
        } else {
            format!("{name}（{count} 拠点）：")
        }
    }

    fn taier_catalog_city_headers(&self) -> [&'static str; 3] {
        ["セレクタ", "都市", "対応キャリア（候補ノード数）"]
    }

    fn taier_no_node(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "選択条件に一致する Taier キャリア拠点がありません".to_owned()
        } else {
            format!("Taier 一覧に {} の該当拠点がありません", unknown.join(", "))
        }
    }

    fn taier_no_result(&self) -> String {
        "いずれの中国キャリア拠点からも速度を取得できませんでした".to_owned()
    }
}

impl CloudflareWords for Japanese {
    fn cloudflare_method(&self, choice: &CloudflareChoice) -> String {
        let mut method = format!(
            "Cloudflare 回線速度：{} ワーカー、片道 {} 秒、HTTPS",
            choice.concurrency, choice.seconds
        );
        if choice.udp {
            method.push_str(&format!(" + STUN UDP（{} プローブ）", choice.udp_packets));
        } else {
            method.push_str("；STUN UDP 省略");
        }
        if choice.custom_endpoints {
            method.push_str("；カスタムエンドポイント");
        }
        method
    }

    fn cloudflare_headers(&self) -> [&'static str; 3] {
        ["指標", "IPv4", "IPv6"]
    }

    fn cloudflare_field(&self, field: CloudflareField) -> &'static str {
        match field {
            CloudflareField::Server => "エッジ拠点",
            CloudflareField::ClientIp => "クライアント IP",
            CloudflareField::ClientAsn => "クライアント ASN",
            CloudflareField::IdleLatency => "無負荷時遅延",
            CloudflareField::Download => "下り速度",
            CloudflareField::DownloadLatency => "下り高負荷遅延",
            CloudflareField::Upload => "上り速度",
            CloudflareField::UploadLatency => "上り高負荷遅延",
            CloudflareField::UdpLoss => "UDP パケットロス",
            CloudflareField::Quality => "通信品質評価",
        }
    }

    fn cloudflare_latency_cell(
        &self,
        median: &str,
        min: &str,
        max: &str,
        jitter: Option<&str>,
        loss_pct: f64,
    ) -> String {
        let mut text = format!("{median} (最小 {min}, 最大 {max}");
        if let Some(jit) = jitter {
            text.push_str(&format!(", ジッタ {jit}"));
        }
        if loss_pct > 0.0 {
            text.push_str(&format!(", ロス {loss_pct:.1}%"));
        }
        text.push(')');
        text
    }

    fn cloudflare_speed_cell(&self, speed: &str, p50_mbps: f64, p95_mbps: f64) -> String {
        format!("{speed} (p50 {p50_mbps:.1}, p95 {p95_mbps:.1} Mbit/s)")
    }

    fn cloudflare_udp_quality(&self, code: &str) -> &'static str {
        match code {
            "excellent" => "最良",
            "good" => "良好",
            "acceptable" => "許容範囲",
            "poor" => "やや不良",
            "bad" => "不良",
            _ => "不明",
        }
    }

    fn cloudflare_udp_cell(
        &self,
        loss_pct: f64,
        received: u64,
        sent: u64,
        reordered_pct: f64,
        mos: Option<f64>,
        quality: &str,
    ) -> String {
        let mut text = format!("{loss_pct:.1}% ({received}/{sent}");
        if reordered_pct > 0.0 {
            text.push_str(&format!(", 順序逆転 {reordered_pct:.1}%"));
        }
        if let Some(m) = mos {
            text.push_str(&format!(", MOS {m:.2}"));
        }
        text.push_str(&format!(", {quality})"));
        text
    }

    fn cloudflare_quality_cell(
        &self,
        bloat_grade: &str,
        bloat_ms: Option<f64>,
        stab_grade: &str,
        stab_cv: Option<f64>,
    ) -> String {
        let bloat = match bloat_ms {
            Some(ms) => format!("バッファブロート {bloat_grade} (+{ms:.1} ms)"),
            None => format!("バッファブロート {bloat_grade}"),
        };
        let stab = match stab_cv {
            Some(cv) => format!("安定性 {stab_grade} (CV {cv:.1}%)"),
            None => format!("安定性 {stab_grade}"),
        };
        format!("{bloat}、{stab}")
    }

    fn cloudflare_issue(&self, issue: CloudflareIssueRef<'_>) -> String {
        match issue {
            CloudflareIssueRef::EmptyDownload => "下り測定バイト数が 0 です".to_owned(),
            CloudflareIssueRef::EmptyUpload => "上り測定バイト数が 0 です".to_owned(),
            CloudflareIssueRef::NoAddress => "該当ファミリのアドレスなし".to_owned(),
            CloudflareIssueRef::TimedOut => "タイムアウト".to_owned(),
            CloudflareIssueRef::NoResolver => "DNS リゾルバなし".to_owned(),
            CloudflareIssueRef::Transport(why) => why.to_owned(),
        }
    }

    fn cloudflare_udp_note(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} STUN UDP 測定不可：{reason}")
    }

    fn cloudflare_family_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} Cloudflare 速度測定失敗：{reason}")
    }

    fn cloudflare_no_result(&self, reason: &str) -> String {
        if reason.is_empty() {
            "Cloudflare 速度測定の結果が得られませんでした".to_owned()
        } else {
            format!("Cloudflare 速度測定失敗：{reason}")
        }
    }
}

impl InetspeedWords for Japanese {
    fn inetspeed_method(&self, choice: &InetspeedChoice) -> String {
        let rounds = if choice.threads <= 1 {
            "1 スレッド".to_owned()
        } else {
            format!("1 および {} スレッド", choice.threads)
        };
        let mut method = format!(
            "Apple CDN (iNetSpeed)：{rounds}、上限 {} / {} 秒、無負荷調査 {} 回",
            choice.max, choice.timeout_secs, choice.latency_count
        );
        if choice.forced_endpoint {
            method.push_str("；固定エッジ IP");
        }
        if choice.custom_endpoints {
            method.push_str("；カスタムエンドポイント");
        }
        method
    }

    fn inetspeed_endpoint_label(&self) -> &'static str {
        "選択エッジ IP"
    }

    fn inetspeed_endpoint_value(
        &self,
        host: &str,
        ip: Option<&str>,
        rtt_ms: Option<f64>,
        source: &str,
    ) -> String {
        let src = self.inetspeed_source(source);
        match (ip, rtt_ms) {
            (Some(addr), Some(rtt)) => format!("{addr} ({host}、{rtt:.2} ms、{src})"),
            (Some(addr), None) => format!("{addr} ({host}、{src})"),
            (None, _) if host.is_empty() => format!("IP 固定なし ({src})"),
            (None, _) => format!("{host} (IP 固定なし、{src})"),
        }
    }

    fn inetspeed_source(&self, code: &str) -> &'static str {
        match code {
            "user" => "手動固定",
            "doh" => "Dual DoH",
            "system_dns" => "システム DNS",
            _ => "既定 DNS",
        }
    }

    fn inetspeed_idle_label(&self) -> &'static str {
        "無負荷時遅延"
    }

    fn inetspeed_latency_value(
        &self,
        median_ms: f64,
        min_ms: f64,
        avg_ms: f64,
        max_ms: f64,
        jitter_ms: f64,
    ) -> String {
        format!(
            "中央値 {median_ms:.2} ms（最小 {min_ms:.2} / 平均 {avg_ms:.2} / \
             最大 {max_ms:.2} ms、ジッタ {jitter_ms:.2} ms）"
        )
    }

    fn inetspeed_total_label(&self) -> &'static str {
        "総転送データ量"
    }

    fn inetspeed_headers(&self) -> [&'static str; 5] {
        ["ラウンド", "スレッド数", "速度", "データ量", "高負荷時遅延"]
    }

    fn inetspeed_round_name(&self, download: bool, single: bool) -> &'static str {
        match (download, single) {
            (true, true) => "下り（シングル）",
            (true, false) => "下り（マルチ）",
            (false, true) => "上り（シングル）",
            (false, false) => "上り（マルチ）",
        }
    }

    fn inetspeed_loaded_cell(&self, median_ms: f64, jitter_ms: f64) -> String {
        format!("{median_ms:.2} ms（ジッタ {jitter_ms:.2} ms）")
    }

    fn inetspeed_round_issue(&self, issue: InetspeedRoundIssueRef<'_>) -> String {
        match issue {
            InetspeedRoundIssueRef::ZeroBytes => "転送完了バイト数が 0 です".to_owned(),
            InetspeedRoundIssueRef::WorkerFaults(count) => {
                format!("{count} ワーカーで通信エラーが発生")
            }
            InetspeedRoundIssueRef::Transport(fault) => inetspeed_fault_text(fault),
        }
    }

    fn inetspeed_warning(&self, warning: InetspeedWarningRef<'_>) -> String {
        match warning {
            InetspeedWarningRef::HostMismatch => {
                "下り・上り・遅延ホストが異なるため共通 IP ピン留めを無効化しました".to_owned()
            }
            InetspeedWarningRef::ForcedProbeFailed(ip) => {
                format!("固定エンドポイント {ip} の事前確認に失敗；指定 IP で続行します")
            }
            InetspeedWarningRef::DohEmpty => {
                "Dual DoH から候補 IP が得られずシステム DNS にフォールバックしました".to_owned()
            }
            InetspeedWarningRef::UnpinnedFallback => {
                "エンドポイント探索で IP が得られず通常の DNS 解決で続行します".to_owned()
            }
            InetspeedWarningRef::NoIdleSamples => {
                "無負荷時遅延のサンプルを取得できませんでした".to_owned()
            }
            InetspeedWarningRef::IdleFailed(fault) => {
                format!("無負荷時遅延の測定失敗：{}", inetspeed_fault_text(fault))
            }
        }
    }

    fn inetspeed_invalid_max(&self, raw: &str) -> String {
        format!("無効な転送上限 {raw:?}：2G、500M、1GiB などのサイズ形式で指定してください")
    }

    fn inetspeed_no_result(&self) -> String {
        "Apple CDN 速度測定で完了したラウンドがありません".to_owned()
    }
}

fn inetspeed_fault_text(fault: InetspeedProbeFaultRef<'_>) -> String {
    match fault {
        InetspeedProbeFaultRef::EmptySample => "サンプル応答なし".to_owned(),
        InetspeedProbeFaultRef::NoAddress => "該当ファミリのアドレスなし".to_owned(),
        InetspeedProbeFaultRef::TimedOut => "タイムアウト".to_owned(),
        InetspeedProbeFaultRef::NoResolver => "DNS リゾルバなし".to_owned(),
        InetspeedProbeFaultRef::Transport(why) => why.to_owned(),
    }
}
