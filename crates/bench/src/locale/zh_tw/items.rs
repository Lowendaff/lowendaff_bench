//! 繁體中文：各測試項目詳情章節文案。

use std::path::Path;

use super::TraditionalChinese;
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

impl CpuWords for TraditionalChinese {
    fn cpu_suite_step(&self, suite: &str, stage: Option<&str>, threads: Option<usize>) -> String {
        let name = self.cpu_suite_title(suite);
        match (stage, threads) {
            (Some(stage), Some(1)) => format!("{name}：{stage}（單並行）"),
            (Some(stage), Some(n)) => format!("{name}：{stage}（{n} 執行緒/並行）"),
            (Some(stage), None) => format!("{name}：{stage}"),
            (None, Some(1)) => format!("{name}：單執行緒測試"),
            (None, Some(n)) => format!("{name}：多執行緒測試（{n} 執行緒）"),
            (None, None) => format!("{name}：執行中"),
        }
    }

    fn cpu_selected_suites(&self, suites: &[String]) -> String {
        format!("啟用子套件：{}", suites.join(", "))
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
            _ => "CPU 子套件",
        }
    }

    fn cpu_suite_skipped(&self, suite: &str, why: &str) -> String {
        format!("{}：已略過（{why}）", self.cpu_suite_title(suite))
    }

    fn cpu_suite_failed(&self, suite: &str, why: &str) -> String {
        format!("{}：失敗（{why}）", self.cpu_suite_title(suite))
    }

    fn cpu_skip_arch(&self, arch: &str) -> String {
        format!("僅支援 x86_64，目前主機架構為 {arch}")
    }

    fn cpu_missing_lib(&self, lib: &str) -> String {
        format!("缺少系統共用程式庫 {lib}")
    }

    fn cpu_setup_failed(&self, reason: &str) -> String {
        format!("無法準備測試工作目錄：{reason}")
    }

    fn sysbench_parameters(&self, version: Option<&str>, max_prime: u64, seconds: u64) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        format!("sysbench{version}：質數上限 {max_prime}，每輪 {seconds} 秒")
    }

    fn cpu_headers(&self) -> [&'static str; 3] {
        ["測試模式", "執行緒數", "事件/秒"]
    }

    fn cpu_row(&self, multi: bool) -> &'static str {
        if multi {
            "多執行緒"
        } else {
            "單執行緒"
        }
    }

    fn cpu_test(&self, multi: bool) -> &'static str {
        if multi {
            "多執行緒測試"
        } else {
            "單執行緒測試"
        }
    }

    fn ratio_label(&self) -> &'static str {
        "多核倍率"
    }

    fn ratio(&self, ratio: f64) -> String {
        format!("多執行緒為單執行緒的 {ratio:.2} 倍")
    }

    fn no_rate(&self) -> String {
        "sysbench 未輸出每秒事件速率".to_owned()
    }

    fn seven_zip_parameters(&self, version: Option<&str>) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        format!("7-Zip{version}：LZMA2 整數壓縮與解壓吞吐")
    }

    fn seven_zip_headers(&self) -> [&'static str; 5] {
        [
            "測試模式",
            "執行緒數",
            "壓縮 (MIPS)",
            "解壓 (MIPS)",
            "總計 (MIPS)",
        ]
    }

    fn seven_zip_no_scores(&self) -> String {
        "7-Zip 未輸出基準評分".to_owned()
    }

    fn unixbench_parameters(&self, version: &str) -> String {
        format!("Byte-UnixBench {version}：12 項系統綜合指數（SPARCstation 20-61 = 10.0）")
    }

    fn unixbench_index_label(&self, copies: usize) -> String {
        if copies <= 1 {
            "綜合指數（單並行）".to_owned()
        } else {
            format!("綜合指數（{copies} 並行）")
        }
    }

    fn unixbench_headers(&self, multi_copies: Option<usize>) -> Vec<String> {
        match multi_copies {
            Some(n) => vec![
                "子項".to_owned(),
                "單並行".to_owned(),
                format!("{n} 並行"),
                "單位".to_owned(),
                "單核指數".to_owned(),
                format!("{n} 核指數"),
            ],
            None => vec![
                "子項".to_owned(),
                "單並行".to_owned(),
                "單位".to_owned(),
                "單核指數".to_owned(),
            ],
        }
    }

    fn unixbench_item_name(&self, id: &str) -> &'static str {
        match id {
            "dhry2reg" => "Dhrystone 2 整數運算",
            "whetstone-double" => "雙精度 Whetstone 浮點",
            "execl" => "Execl 吞吐",
            "fstime" => "檔案複製 (1024 B, 2000 區塊)",
            "fsbuffer" => "檔案複製 (256 B, 500 區塊)",
            "fsdisk" => "檔案複製 (4096 B, 8000 區塊)",
            "pipe" => "管線吞吐",
            "context1" => "管線內容切換",
            "spawn" => "行程建立",
            "shell1" => "Shell 指令碼 (1 並行)",
            "shell8" => "Shell 指令碼 (8 並行)",
            "syscall" => "系統呼叫開銷",
            _ => "UnixBench 子項",
        }
    }

    fn unixbench_no_count(&self, item: &str) -> String {
        format!("UnixBench {item} 未輸出 COUNT 讀數")
    }

    fn unixbench_timed_out(&self, item: &str, seconds: u64) -> String {
        format!("UnixBench {item} 執行超過 {seconds} 秒仍未結束，已終止")
    }

    fn hpl_parameters(&self, version: Option<&str>, n: u32, nb: u32, threads: usize) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        format!("HPL{ver} (OpenBLAS)：稠密雙精度 LU 分解，N={n}，NB={nb}，{threads} 執行緒")
    }

    fn hpl_labels(&self) -> [&'static str; 3] {
        ["浮點算力", "求解耗時", "殘差校驗"]
    }

    fn hpl_gflops(&self, gflops: f64) -> String {
        format!("{gflops:.2} GFLOPS")
    }

    fn hpl_seconds(&self, seconds: f64) -> String {
        format!("{seconds:.2} s")
    }

    fn hpl_residual_passed(&self, residual: Option<f64>) -> String {
        match residual {
            Some(res) => format!("通過 ({res:.2e})"),
            None => "通過".to_owned(),
        }
    }

    fn hpl_residual_failed(&self) -> String {
        "HPL 縮放殘差數值校驗未通過".to_owned()
    }

    fn passmark_parameters(&self, version: Option<&str>) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        format!("PassMark PerformanceTest{ver}：Linux CPU 測試套件")
    }

    fn passmark_summary_labels(&self) -> [&'static str; 2] {
        ["CPU Mark 總分", "單執行緒分數"]
    }

    fn passmark_headers(&self) -> [&'static str; 3] {
        ["細分項目", "分數", "單位"]
    }

    fn passmark_subtests(&self) -> [(&'static str, &'static str); 8] {
        [
            ("整數運算", "MOps/s"),
            ("浮點運算", "MOps/s"),
            ("質數篩選", "MPrimes/s"),
            ("字串排序", "KStrings/s"),
            ("資料加密", "MB/s"),
            ("資料壓縮", "KB/s"),
            ("物理模擬", "Frames/s"),
            ("擴充指令集", "MMatrices/s"),
        ]
    }

    fn passmark_no_scores(&self) -> String {
        "PassMark 未產生 CPU 評分結果".to_owned()
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
        format!("y-cruncher{ver}：圓周率 Pi 計算至 {millions}M 十進位數{tune}")
    }

    fn ycruncher_labels(&self) -> [&'static str; 4] {
        ["純計算耗時", "總掛鐘耗時", "CPU 使用率", "多核並行效率"]
    }

    fn ycruncher_validation_failed(&self) -> String {
        "y-cruncher 圓周率數位抽檢校驗未通過".to_owned()
    }

    fn prime95_parameters(
        &self,
        version: Option<&str>,
        workers: usize,
        duration_secs: u64,
        fft: Option<&str>,
    ) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        let kernel = fft.map_or_else(String::new, |f| format!("，{f}"));
        format!(
            "Prime95{ver}：Small-FFT 穩定性燒機，{workers} 工作執行緒，{duration_secs} 秒{kernel}"
        )
    }

    fn prime95_labels(&self) -> [&'static str; 2] {
        ["穩定性結果", "完成測試項"]
    }

    fn prime95_passed(&self, warnings: u64) -> String {
        format!("通過（0 錯誤，{warnings} 警告）")
    }

    fn prime95_hardware_error(&self, detail: &str) -> String {
        format!("Prime95 偵測到硬體或捨入計算錯誤：{detail}")
    }

    fn prime95_no_summary(&self) -> String {
        "Prime95 離開時未輸出工作執行緒燒機彙總".to_owned()
    }

    fn geekbench_labels(&self) -> [&'static str; 3] {
        ["單核分數", "多核分數", "結果連結"]
    }

    fn swapped(&self, need: u64, available: &str, swap: u64) -> String {
        format!(
            "可用記憶體不足（約需 {need} GiB，目前可用 {available} GiB）；\
             已自動掛載 {swap} GiB 暫存 Swap，分數可能偏低"
        )
    }

    fn memory_short(&self, need: u64, available: &str, why: NoSwapWhy<'_>) -> String {
        let why = match why {
            NoSwapWhy::Unused => "目前 vm.swappiness 為 0，核心不會換出記憶體".to_owned(),
            NoSwapWhy::NoRoom(Some(free)) => {
                format!("執行目錄所在磁碟剩餘空間僅 {free} GiB")
            }
            NoSwapWhy::NoRoom(None) => "無法讀取執行目錄所在磁碟剩餘空間".to_owned(),
            NoSwapWhy::Denied => "權限不足（容器環境通常禁止掛載 Swap）".to_owned(),
            NoSwapWhy::Unsupported => "執行目錄所在檔案系統不支援 Swap 檔案".to_owned(),
            NoSwapWhy::Error(error) => error.to_owned(),
        };
        format!(
            "可用記憶體可能不足（約需 {need} GiB，目前可用 {available} GiB）；\
             未啟用暫存 Swap：{why}"
        )
    }

    fn first_to_go_not_set(&self, error: &str) -> String {
        format!("無法為 Geekbench 設定記憶體耗盡優先回收策略：{error}")
    }

    fn oom_not_restored(&self, error: &str) -> String {
        format!("無法還原主程式的 oom_score_adj：{error}")
    }

    fn uploads_retried(&self, count: usize, last: &str) -> String {
        format!("經歷 {count} 次上傳失敗後重新執行成功；上次原因：{last}")
    }

    fn swap_left(&self, error: &str, path: &Path) -> String {
        format!(
            "無法卸載暫存 Swap：{error}；請手動執行 swapoff {} 並刪除該檔案",
            path.display()
        )
    }

    fn upload_failed(&self, reason: &str) -> String {
        format!("成績上傳失敗（{reason}）；免費版 Geekbench 需成功上傳後才回傳分數")
    }

    fn no_scores(&self) -> String {
        "Geekbench 未輸出跑分成績".to_owned()
    }

    fn out_of_memory(&self) -> String {
        "執行期間因系統記憶體耗盡（OOM）被終止".to_owned()
    }

    fn cpu_no_suites_selected(&self) -> String {
        "未選擇任何 CPU 基準測試子套件".to_owned()
    }

    fn cpu_all_skipped(&self) -> String {
        "所選全部 CPU 子套件在目前主機上均不適用而已略過".to_owned()
    }

    fn cpu_all_failed(&self) -> String {
        "所有可執行的 CPU 基準測試子套件均執行失敗".to_owned()
    }
}

impl MemoryWords for TraditionalChinese {
    fn memory_sysbench(
        &self,
        version: Option<&str>,
        seconds: u64,
        single: &str,
        per_thread: Option<&str>,
    ) -> String {
        let mut text = format!(
            "{}循序讀寫，每輪 {seconds} 秒；單執行緒緩衝區 {single}",
            self::version(version)
        );
        if let Some(per_thread) = per_thread {
            text.push_str(&format!("，多執行緒每執行緒 {per_thread}"));
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
            "；執行緒均勻分布於各 NUMA 節點"
        } else {
            ""
        };
        format!(
            "{}單陣列 {millions}00 萬元素（{array} MiB，三組共 {arrays} MiB），\
             每項取 10 輪最快值{spread}",
            self::version(version)
        )
    }

    fn latency_label(&self) -> &'static str {
        "存取延遲"
    }

    fn latency_method(&self) -> String {
        "multichase：隨機指標追蹤，每級測試 2.5 秒".to_owned()
    }

    fn numa_method(&self, nodes: usize) -> String {
        format!(
            "{nodes} 個節點；頻寬採用 multiload stream-triad（每格取 3 次最快值），\
             延遲測量同上"
        )
    }

    fn results(&self, tool: &str) -> String {
        format!("{tool} 測試結果")
    }

    fn numa_title(&self, probe: Probe) -> &'static str {
        match probe {
            Probe::Bandwidth => "NUMA 記憶體頻寬（MiB/s；列：執行緒所在節點，欄：記憶體所在節點）",
            Probe::Latency => "NUMA 存取延遲 (ns)",
        }
    }

    fn test_header(&self) -> &'static str {
        "測試項目"
    }

    fn threads_column(&self, threads: usize) -> String {
        if threads > 1 {
            format!("多執行緒（{threads} 執行緒）")
        } else {
            "單執行緒".to_owned()
        }
    }

    fn latency_headers(&self) -> [&'static str; 3] {
        ["層級", "工作集大小", "延遲"]
    }

    fn node(&self, node: usize) -> String {
        format!("節點 {node}")
    }

    fn operation(&self, write: bool) -> &'static str {
        if write { "寫入" } else { "讀取" }
    }

    fn memory_level(&self) -> &'static str {
        "主記憶體"
    }

    fn cache_unknown(&self) -> String {
        "未讀取到末級快取容量，按 32 MiB 估算".to_owned()
    }

    fn stream_rule(&self) -> String {
        "受可用記憶體限制，單陣列小於末級快取 4 倍（未完全滿足 STREAM 標準規則）".to_owned()
    }

    fn sysbench_small(&self, four: &str) -> String {
        format!(
            "可用記憶體較少：測試資料小於末級快取 4 倍（{four}），\
             部分讀寫命中快取，結果可能偏高"
        )
    }

    fn latency_small(&self, four: &str) -> String {
        format!(
            "可用記憶體較少：主記憶體測試區小於末級快取 4 倍（{four}），\
             部分讀取命中快取，延遲可能偏低"
        )
    }

    fn levels_missing(&self, levels: &[&str]) -> String {
        match levels {
            [level] => format!("未取得 {level} 容量，已略過該層級"),
            [others @ .., last] => {
                format!(
                    "未取得 {} 與 {last} 容量，已略過上述層級",
                    others.join("、")
                )
            }
            [] => String::new(),
        }
    }

    fn sysbench_run(&self, write: bool, multi: bool) -> String {
        let threads = if multi {
            "多執行緒"
        } else {
            "單執行緒"
        };
        let operation = if write { "寫入" } else { "讀取" };
        format!("{threads}{operation}")
    }

    fn stream_run(&self, multi: bool) -> String {
        if multi {
            "多執行緒測試".to_owned()
        } else {
            "單執行緒測試".to_owned()
        }
    }

    fn latency_run(&self, level: &str) -> String {
        format!("{level} 測試")
    }

    fn numa_run(&self, row: usize, column: usize, probe: Probe) -> String {
        let test = match probe {
            Probe::Bandwidth => "頻寬",
            Probe::Latency => "延遲",
        };
        format!("節點 {row} 執行緒存取節點 {column} 記憶體的{test}測試")
    }

    fn no_transfer_rate(&self) -> String {
        "sysbench 未輸出記憶體傳輸速率".to_owned()
    }

    fn stream_invalid(&self, line: &str) -> String {
        format!("STREAM 結果校驗未通過：{line}")
    }

    fn no_result(&self, tool: &str) -> String {
        format!("{tool} 未輸出測試結果")
    }

    fn no_latency(&self) -> String {
        "multichase 未輸出延遲讀數".to_owned()
    }

    fn no_bandwidth(&self) -> String {
        "multiload 未輸出頻寬讀數".to_owned()
    }

    fn bind_refused(&self) -> String {
        "目前容器環境禁止將記憶體綁定至指定 NUMA 節點".to_owned()
    }

    fn no_memory_info(&self) -> String {
        "無法讀取系統可用記憶體（/proc/meminfo）".to_owned()
    }

    fn stream_short(&self, needed: u64, budget: &str) -> String {
        format!(
            "可用記憶體不足：三組陣列至少需要 {needed} MiB，\
             而可用記憶體的一半僅為 {budget}"
        )
    }

    fn one_node(&self) -> String {
        "需要多個 NUMA 節點".to_owned()
    }
}

impl DiskWords for TraditionalChinese {
    fn health_headers(&self) -> [&'static str; 8] {
        [
            "裝置",
            "健康狀態",
            "通電時數",
            "溫度",
            "耗損率",
            "累計寫入",
            "壞軌數",
            "連線錯誤",
        ]
    }

    fn card_disk(&self, numbers: &str, host: Option<u32>) -> String {
        match host {
            Some(host) => format!("RAID 卡 host{host} 磁碟 {numbers}"),
            None => format!("RAID 卡磁碟 {numbers}"),
        }
    }

    fn verdict(&self, verdict: Verdict) -> &'static str {
        match verdict {
            Verdict::Passed => "正常",
            Verdict::Failed => "異常",
            Verdict::Unknown => "未知",
            Verdict::Unsupported => "不支援",
            Verdict::ReadFailed => "讀取失敗",
        }
    }

    fn hours(&self, hours: u64) -> String {
        format!("{hours} 小時")
    }

    fn disk_read_failed(&self, disk: &str, reason: &str) -> String {
        format!("{disk}：讀取失敗：{reason}")
    }

    fn no_smart(&self, disk: &str) -> String {
        format!("{disk} 不支援 SMART")
    }

    fn smart_off(&self, disk: &str) -> String {
        format!("{disk} 未開啟 SMART（本程式不會主動修改磁碟設定）")
    }

    fn disk_read_timed_out(&self, seconds: u64) -> String {
        format!("讀取超過 {seconds} 秒仍未結束，已終止")
    }

    fn unparsable(&self) -> String {
        "無法解析 smartctl 的輸出".to_owned()
    }

    fn unopened(&self) -> String {
        "無法開啟磁碟裝置或讀取其識別資訊".to_owned()
    }

    fn arguments_refused(&self) -> String {
        "smartctl 拒絕了命令列參數".to_owned()
    }

    fn node_left(&self, path: &Path, error: &str) -> String {
        format!("無法刪除暫存裝置節點 {}：{error}", path.display())
    }

    fn no_disks(&self) -> String {
        "smartctl 未發現任何實體磁碟".to_owned()
    }

    fn scan_timed_out(&self, seconds: u64) -> String {
        format!("smartctl 掃描磁碟失敗：超過 {seconds} 秒仍未結束")
    }

    fn scan_failed(&self, reason: &str) -> String {
        format!("smartctl 掃描磁碟失敗：{reason}")
    }

    fn all_failed(&self) -> String {
        "所有磁碟均無法讀取 SMART 資訊".to_owned()
    }

    fn fio_method(&self, size: &str) -> (&'static str, String) {
        (
            "測試方法",
            format!("對齊 YABS 標準：50% 讀 + 50% 寫混合隨機 I/O，測試檔案 {size}"),
        )
    }

    fn zfs_note(&self, needed: u64) -> String {
        format!(
            "測試目錄位於 ZFS 且剩餘可用空間不足 {needed} GiB，\
             測試成績可能受壓縮/快取影響（同 YABS 提示）"
        )
    }

    fn block_sizes(&self, first: &str, second: &str) -> String {
        format!("{first} 與 {second}")
    }

    fn block_size_header(&self) -> &'static str {
        "區塊大小"
    }

    fn fio_row(&self, row: FioRow) -> &'static str {
        match row {
            FioRow::Read => "讀取",
            FioRow::Write => "寫入",
            FioRow::Total => "合計",
        }
    }

    fn rate(&self, speed: &str, iops: &str) -> String {
        format!("{speed} ({iops})")
    }

    fn fio_timed_out(&self) -> &'static str {
        "逾時"
    }

    fn fio_error(&self) -> &'static str {
        "錯誤"
    }

    fn block_timed_out(&self, block_size: &str, seconds: u64) -> String {
        format!("{block_size} 測試超過 {seconds} 秒仍未結束，已終止")
    }

    fn block_failed(&self, block_size: &str, reason: &str) -> String {
        format!("{block_size} 測試失敗：{reason}")
    }

    fn no_space_info(&self) -> String {
        "無法讀取測試目錄的可用磁碟空間".to_owned()
    }

    fn in_memory(&self) -> String {
        "測試目錄位於記憶體檔案系統（如 tmpfs），這會測到記憶體而非硬碟；\
         請在磁碟目錄執行或用 --work-dir 指定掛載路徑"
            .to_owned()
    }

    fn no_space(&self, needed: &str, available: &str) -> String {
        format!("測試目錄可用空間不足 {needed}（目前僅剩 {available}）")
    }

    fn no_direct_io(&self) -> String {
        "測試目錄所在檔案系統不支援直接 I/O（O_DIRECT）".to_owned()
    }

    fn setup_failed(&self, reason: &str) -> String {
        format!("無法建立測試檔案：{reason}")
    }

    fn no_block_size(&self) -> String {
        "四種區塊大小均未取得有效測試成績".to_owned()
    }

    fn no_output(&self, tool: &str) -> String {
        format!("{tool} 未產生任何輸出")
    }

    fn fio_reported(&self, error: &str) -> String {
        format!("fio 回報錯誤碼 {error}")
    }

    fn fio_field(&self, field: usize) -> String {
        format!("無法解析 fio 輸出的第 {field} 個欄位")
    }
}

impl NetworkWords for TraditionalChinese {
    fn iperf3_method(
        &self,
        version: Option<&str>,
        streams: u8,
        seconds: u16,
        rate: Option<u32>,
    ) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        let rate = rate.map_or_else(String::new, |rate| format!("，單流限速 {rate} Mbit/s"));
        format!("iperf3{version}：{streams} 並行串流，單向 {seconds} 秒，TCP{rate}")
    }

    fn family_title(&self, ipv6: bool) -> &'static str {
        if ipv6 { "IPv6" } else { "IPv4" }
    }

    fn network_headers(&self) -> [&'static str; 5] {
        ["節點位置", "服務商", "上傳速度", "下載速度", "延遲"]
    }

    fn place(&self, location: &str, country: &str) -> String {
        format!("{location}, {country}")
    }

    fn way_cell(&self, cell: WayCell) -> &'static str {
        match cell {
            WayCell::Busy => "忙碌",
            WayCell::Unreachable => "無法連線",
            WayCell::TimedOut => "逾時",
            WayCell::Error => "錯誤",
            WayCell::NotTried => "未嘗試",
        }
    }

    fn tries(&self, place: &str, download: bool, tries: u32) -> String {
        let direction = if download { "下載" } else { "上傳" };
        format!("{place}：{direction}測速共嘗試 {tries} 次")
    }

    fn way_failed(&self, place: &str, download: bool, reason: &str) -> String {
        let direction = if download { "下載" } else { "上傳" };
        format!("{place}：{direction}測速失敗：{reason}")
    }

    fn nothing_measured(&self) -> String {
        "iperf3 未測得任何有效資料".to_owned()
    }

    fn unknown_servers(&self, names: &[String]) -> String {
        format!("節點清單中不存在以下伺服器：{}", names.join(", "))
    }

    fn list_unreadable(&self, reason: &str) -> String {
        format!("無法讀取測速節點清單：{reason}")
    }

    fn list_too_large(&self, limit: usize) -> String {
        format!("測速節點清單超過 {limit} 位元組上限")
    }

    fn list_format(&self, reason: &str) -> String {
        format!("測速節點清單格式無效：{reason}")
    }

    fn list_newer(&self, version: u64) -> String {
        format!("測速節點清單需要更新版本的 lowendaff-bench（清單格式版本 {version}）")
    }

    fn list_version(&self, version: u64) -> String {
        format!("測速節點清單宣告了不存在的格式版本：{version}")
    }

    fn list_too_many(&self, count: usize, most: usize) -> String {
        format!("測速節點清單包含 {count} 個節點，超出上限 {most}")
    }

    fn list_rule(&self, position: usize, rule: ListRule) -> String {
        let rule = match rule {
            ListRule::Id => "id 必須由 1 至 40 個小寫字母、數字或連字號組成",
            ListRule::Country => "country 必須為兩位大寫字母國家代碼",
            ListRule::Host => "host 必須為網域名稱或 IP 位址",
            ListRule::Ports => "ports 必須為 1024..=65535 內的單一連接埠或最多 64 個連接埠範圍",
            ListRule::Families => "families 必須為 ipv4、ipv6 或 both",
            ListRule::Text => "location、provider 與 link 必須為 1 至 40 個可列印字元",
        };
        format!("測速節點清單第 {position} 項違反驗證規則：{rule}")
    }

    fn list_duplicate(&self, id: &str) -> String {
        format!("測速節點清單中存在重複的節點 ID：{id}")
    }

    fn no_server(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "未選中任何測速節點".to_owned()
        } else {
            format!("節點清單中沒有相符於 {} 的測速節點", unknown.join(", "))
        }
    }

    fn no_family(&self) -> String {
        "所選測速節點均不支援目前主機可用的 IP 協定棧".to_owned()
    }

    fn no_speed(&self) -> String {
        "所有測速節點均未回傳有效頻寬結果".to_owned()
    }
}

impl TraceWords for TraditionalChinese {
    fn trace_method(&self, version: &str, choice: &TraceChoice) -> String {
        let mut method = format!("NextTrace {version}：ICMP，每跳 3 次探測，最多 30 跳");
        if choice.tcp {
            method.push_str(&format!(
                "；中國大陸目標附加 {} 位元組 TCP 大封包探測",
                choice.large_packets
            ));
        }
        if !choice.groups.is_empty() {
            method.push_str(&format!("；分組 {}", choice.groups.join(", ")));
        }
        if !choice.targets.is_empty() {
            method.push_str(&format!("；指定目標 {}", choice.targets.join(", ")));
        }
        if choice.list {
            method.push_str("；使用自訂目標清單");
        }
        if !choice.geo {
            method.push_str("；不查詢網路與地理位置");
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
        format!("TCP 連接埠 {port}（{packet_size} 位元組探測封包）：{outcome}")
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
        format!("共 {hops} 跳到達")
    }

    fn not_reached(&self, hops: usize) -> String {
        format!("經 {hops} 跳仍未到達目標")
    }

    fn fragmented(&self, hop: usize, mtu: Option<u16>) -> String {
        match mtu {
            Some(mtu) => format!("在第 {hop} 跳受阻（下一跳路由器最大允許 {mtu} 位元組）"),
            None => format!("在第 {hop} 跳受阻（路由器要求分片，無法通過該大封包）"),
        }
    }

    fn trace_headers(&self) -> [&'static str; 6] {
        ["跳數", "位址", "ASN", "網路", "位置", "延遲"]
    }

    fn trace_group_title(&self, code: &str) -> &'static str {
        match code {
            "services" => "公共內容節點",
            "china" => "中國大陸線路",
            "international" => "國際骨幹與雲端節點",
            _ => "路由追蹤",
        }
    }

    fn trace_md_target_title(&self, name: &str, ipv6: bool) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{name} ({fam})")
    }

    fn trace_md_target_line(&self, host: Option<&str>, outcome: &str) -> String {
        match host {
            Some(host) => format!("目標位址：{host}（{outcome}）"),
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
        format!("不可用：{reason}")
    }

    fn lookup_problem(&self, problem: LookupProblem) -> String {
        match problem {
            LookupProblem::NoServer => "Google 介面未回傳快取節點",
            LookupProblem::Id => "Google 回傳的節點 ID 無法組裝為合法網域名稱",
            LookupProblem::Format => "fast.com 回傳內容不符合 API 格式",
            LookupProblem::NoTarget => "fast.com 未回傳測速快取節點",
            LookupProblem::Url => "fast.com 回傳的節點位址無法用於路由追蹤",
            LookupProblem::Place => "fast.com 回傳的節點位置無法顯示",
        }
        .to_owned()
    }

    fn answer_unreadable(&self, reason: &str) -> String {
        format!("無法讀取回應內容：{reason}")
    }

    fn traced_nothing(&self) -> String {
        "NextTrace 未回傳任何路由跳點".to_owned()
    }

    fn unknown_targets(&self, ids: &[String]) -> String {
        format!("路由目標清單中不存在：{}", ids.join(", "))
    }

    fn no_exchanges(&self, reason: &str) -> String {
        format!("未載入 IX 交換中心清單：{reason}")
    }

    fn trace_list_problem(&self, list: TraceList, problem: TraceListProblem<'_>) -> String {
        let (name, entry) = match list {
            TraceList::Targets => ("路由目標清單", "目標"),
            TraceList::Exchanges => ("IX 交換中心連接埠清單", "連接埠"),
        };
        match problem {
            TraceListProblem::Unreadable(reason) => format!("無法讀取{name}：{reason}"),
            TraceListProblem::TooLarge(limit) => format!("{name}超過 {limit} 位元組上限"),
            TraceListProblem::Format(reason) => format!("{name}格式無效：{reason}"),
            TraceListProblem::Newer(version) => {
                format!("{name}需要更新版本的 lowendaff-bench（格式版本 {version}）")
            }
            TraceListProblem::Version(version) => {
                format!("{name}宣告了不存在的格式版本：{version}")
            }
            TraceListProblem::TooMany(count, most) => {
                format!("{name}包含 {count} 個{entry}，超出上限 {most}")
            }
            TraceListProblem::Rule(position, rule) => {
                let rule = match rule {
                    TraceRule::Id => {
                        "id 必須為 1 至 40 個小寫字母、數字或 -，且不得為 youtube 或 netflix"
                    }
                    TraceRule::Group => "group 必須為 china 或 international",
                    TraceRule::Name => "name 必須為 1 至 40 個可列印字元",
                    TraceRule::Asn => "ASN 必須在 1..=4294967295 範圍內",
                    TraceRule::Ipv4 => "ipv4 必須為網域名稱或 IPv4 位址",
                    TraceRule::Ipv6 => "ipv6 必須為網域名稱或 IPv6 位址",
                    TraceRule::Tcp => "tcp 連接埠僅允許設定於 china 目標上，範圍 1..=65535",
                    TraceRule::Address => "address 必須為標準格式的 IPv4 或純 IPv6 位址",
                    TraceRule::Exchange => "exchange 必須為 1 至 60 個可列印字元",
                };
                format!("{name}第 {position} 個{entry}違反規則：{rule}")
            }
            TraceListProblem::Duplicate(key) => match list {
                TraceList::Targets => format!("{name}存在重複的目標 ID：{key}"),
                TraceList::Exchanges => format!("{name}存在重複的連接埠位址：{key}"),
            },
        }
    }

    fn no_target(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "未選中任何路由追蹤目標".to_owned()
        } else {
            format!("目標清單中不存在：{}", unknown.join(", "))
        }
    }

    fn no_target_family(&self) -> String {
        "所選路由目標均不包含目前主機可用的 IP 協定棧位址".to_owned()
    }

    fn no_target_route(&self) -> String {
        "所有路由目標均未取得追蹤結果".to_owned()
    }
}

impl MediaWords for TraditionalChinese {
    fn media_method(&self, choice: &MediaChoice) -> String {
        let mut method = if choice.custom_base {
            "自訂探測基準 URL，直連探測（不使用代理）".to_owned()
        } else {
            "HTTPS 直連探測（不使用代理）".to_owned()
        };
        if !choice.groups.is_empty() {
            method.push_str(&format!("；區域 {}", choice.groups.join(", ")));
        }
        if !choice.services.is_empty() {
            method.push_str(&format!("；指定平台 {}", choice.services.join(", ")));
        }
        method
    }

    fn media_group(&self, code: &str) -> &'static str {
        match code {
            "global" => "跨國通用平台",
            "asia" => "亞太地區",
            "europe" => "歐洲地區",
            "americas" => "美洲地區",
            "oceania" => "大洋洲地區",
            _ => "其他地區",
        }
    }

    fn media_headers(&self) -> [&'static str; 3] {
        ["串流媒體 / 服務", "IPv4", "IPv6"]
    }

    fn media_yes(&self, region: Option<&str>) -> String {
        match region {
            Some(region) => format!("解鎖 ({region})"),
            None => "解鎖".to_owned(),
        }
    }

    fn media_originals(&self, region: Option<&str>) -> String {
        match region {
            Some(region) => format!("僅自製劇 ({region})"),
            None => "僅自製劇".to_owned(),
        }
    }

    fn media_cell(&self, cell: MediaCell, ipv6: bool) -> &'static str {
        match cell {
            MediaCell::No => "未解鎖",
            MediaCell::NoChina => "不可用 (CN)",
            MediaCell::ComingSoon => "即將推出",
            MediaCell::ProxyDetected => "未解鎖 (偵測代理)",
            MediaCell::BlockedWaf => "遭攔截 (WAF)",
            MediaCell::RateLimited => "請求限流",
            MediaCell::NoAddress => {
                if ipv6 {
                    "無 IPv6 解析"
                } else {
                    "無 IPv4 解析"
                }
            }
            MediaCell::TimedOut => "逾時",
            MediaCell::Error => "探測異常",
        }
    }

    fn media_unexpected(&self, service: &str, ipv6: bool, reason: &str) -> String {
        let family = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{service} ({family})：{reason}")
    }

    fn unknown_services(&self, ids: &[String]) -> String {
        format!("串流媒體目錄中不存在以下服務：{}", ids.join(", "))
    }

    fn invalid_base_url(&self, url: &str) -> String {
        format!(
            "base_url 必須以 https:// 或本機回送位址 http://127.0.0.1 / http://[::1] 開頭：{url}"
        )
    }

    fn no_service(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "未選擇任何串流媒體平台".to_owned()
        } else {
            format!("串流媒體目錄中不存在：{}", unknown.join(", "))
        }
    }

    fn no_media_family(&self) -> String {
        "目前主機既無 IPv4 也無 IPv6 公網連線".to_owned()
    }
}

impl IpWords for TraditionalChinese {
    fn ip_method(&self, choice: &IpChoice) -> String {
        let mut parts = vec!["9 家情報源"];
        if choice.dnsbl {
            parts.push("DNSBL 黑名單");
        }
        if choice.mail {
            parts.push("25 連接埠郵局");
        }
        let mut method = format!("直連檢測（不使用代理）：{}", parts.join("、"));
        if choice.custom_endpoints {
            method.push_str("；自訂介面位址");
        }
        method
    }

    fn ip_stage(&self, stage: &str) -> &'static str {
        match stage {
            "intelligence" => "威脅情報與資料庫",
            "dnsbl" => "DNSBL 黑名單",
            "mail" => "25 連接埠郵局連通性",
            _ => "IP 品質",
        }
    }

    fn ip_table(&self, table: IpTable) -> &'static str {
        match table {
            IpTable::Summary => "基本資訊",
            IpTable::Scores => "風控與詐欺評分",
            IpTable::Factors => "風險標籤統計",
            IpTable::BlacklistsAndMail => "黑名單與 25 郵件連接埠",
        }
    }

    fn ip_headers(&self, table: IpTable) -> [&'static str; 3] {
        let first = match table {
            IpTable::Summary => "屬性",
            IpTable::Scores => "資料來源",
            IpTable::Factors => "風險標籤",
            IpTable::BlacklistsAndMail => "檢測項目",
        };
        [first, "IPv4", "IPv6"]
    }

    fn ip_field(&self, field: IpField) -> &'static str {
        match field {
            IpField::Address => "公網位址",
            IpField::Hostname => "反向網域",
            IpField::Asn => "ASN",
            IpField::Organization => "所屬機構",
            IpField::Registry => "註冊局",
            IpField::Origin => "IP 來源",
            IpField::UsageType => "使用類型",
            IpField::GeoIp => "地理位置",
        }
    }

    fn ip_native(&self, country: &str) -> String {
        format!("原生 IP ({country})")
    }

    fn ip_broadcast(&self, registered: &str, located: &str) -> String {
        format!("廣播 IP ({registered} -> {located})")
    }

    fn ip_usage(&self, code: &str) -> &'static str {
        match code {
            "residential" => "家庭寬頻",
            "hosting" => "資料中心 / 機房",
            "business" => "企業專線",
            "mobile" => "行動網路",
            "education" => "學術研究網",
            "government" => "政府機構",
            "cdn" => "CDN 節點",
            _ => "未知",
        }
    }

    fn ip_risk_level(&self, code: &str) -> &'static str {
        match code {
            "low" => "低風險",
            "medium" => "中等風險",
            "high" => "高風險",
            "very_high" => "極高風險",
            _ => "未知",
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
            return "已知良性服務 (RIOT)";
        }
        match class {
            "not_observed" if !noise => "正常（未見全網掃描）",
            "benign" => "良性掃描源",
            "malicious" => "惡意掃描源",
            _ if noise => "觀測到掃描行為",
            _ => "正常（未見全網掃描）",
        }
    }

    fn ip_shodan(&self, ports: usize, vulns: usize, flagged: bool) -> String {
        let label = if flagged || vulns > 0 {
            "存在標記"
        } else {
            "正常"
        };
        format!("{label}（{ports} 個開放連接埠，{vulns} 個 CVE）")
    }

    fn ip_factor_name(&self, code: &str) -> &'static str {
        match code {
            "proxy" => "代理 (Proxy)",
            "vpn" => "VPN",
            "tor" => "Tor 節點",
            "datacenter" => "機房 (IDC)",
            "abuser" => "濫用紀錄",
            "robot" => "自動化爬蟲",
            "bogon" => "保留位址 (Bogon)",
            _ => "其他",
        }
    }

    fn ip_factor_cell(&self, checked: usize, flagged: &[&str]) -> String {
        if flagged.is_empty() {
            format!("否 (0/{checked})")
        } else {
            format!("是 ({}/{checked}：{})", flagged.len(), flagged.join(", "))
        }
    }

    fn ip_dnsbl_label(&self) -> &'static str {
        "DNSBL 黑名單"
    }

    fn ip_dnsbl_cell(&self, checked: usize, refused: usize, listed: &[&str]) -> String {
        if listed.is_empty() {
            if refused > 0 {
                format!("正常 (0/{checked}；{refused} 個拒絕查詢)")
            } else {
                format!("正常 (0/{checked})")
            }
        } else {
            format!("命中 ({}/{checked}：{})", listed.len(), listed.join(", "))
        }
    }

    fn ip_port25_label(&self) -> &'static str {
        "出站 25 連接埠"
    }

    fn ip_port25_cell(&self, open: bool) -> &'static str {
        if open { "開放" } else { "封鎖" }
    }

    fn ip_mail_yes(&self, code: u16) -> String {
        format!("連通 ({code})")
    }

    fn ip_mail_rejected(&self, code: u16) -> String {
        format!("拒收 ({code})")
    }

    fn ip_mail_blocked(&self) -> &'static str {
        "封鎖 / 逾時"
    }

    fn ip_unexpected(&self, provider: &str, ipv6: bool, reason: &str) -> String {
        let family = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{provider} ({family})：{reason}")
    }

    fn ip_no_public_address(&self) -> String {
        "未取得任何可用協定棧的公網 IP 位址".to_owned()
    }

    fn ip_invalid_endpoint(&self, raw: &str) -> String {
        format!("介面覆寫位址必須為 https:// 或本機回送位址 (127.0.0.1 / [::1])：{raw}")
    }
}

impl BgpWords for TraditionalChinese {
    fn bgp_method(&self, custom_endpoints: bool, upload: bool) -> String {
        let mut line = "RIPEstat RIS 直連查詢：BGP 路由宣告、RPKI 與 AS 路徑".to_owned();
        if !upload {
            line.push_str("；不上傳拓撲圖片");
        }
        if custom_endpoints {
            line.push_str("；自訂介面位址");
        }
        line
    }

    fn bgp_summary_title(&self) -> &'static str {
        "BGP 路由概覽"
    }

    fn bgp_summary_headers(&self) -> [&'static str; 3] {
        ["欄位", "IPv4", "IPv6"]
    }

    fn bgp_field(&self, field: BgpField) -> &'static str {
        match field {
            BgpField::Address => "公網位址",
            BgpField::Prefix => "宣告前綴",
            BgpField::OriginAsn => "起源 ASN",
            BgpField::Rpki => "RPKI 狀態",
            BgpField::Visibility => "RIS 可見度",
            BgpField::Upstreams => "直連上游",
        }
    }

    fn bgp_not_announced(&self) -> &'static str {
        "未見 BGP 宣告"
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
            "valid" => "有效 (Valid)",
            "invalid" => "無效 (Invalid)",
            "not_found" => "未簽發 (無 ROA)",
            _ => "未知",
        }
    }

    fn bgp_visibility_cell(&self, peers: u32, distinct_paths: usize) -> String {
        format!("{peers} 個收集節點，{distinct_paths} 條獨立路徑")
    }

    fn bgp_direct_peering(&self) -> &'static str {
        "與收集器直連對等"
    }

    fn bgp_topology_title(&self, ipv6: bool) -> &'static str {
        if ipv6 {
            "IPv6 上游拓撲"
        } else {
            "IPv4 上游拓撲"
        }
    }

    fn bgp_paths_title(&self, ipv6: bool) -> &'static str {
        if ipv6 {
            "IPv6 觀測路徑"
        } else {
            "IPv4 觀測路徑"
        }
    }

    fn bgp_paths_headers(&self) -> [&'static str; 3] {
        ["觀測節點數", "跳數", "AS_PATH"]
    }

    fn bgp_issue(&self, issue: BgpIssueRef<'_>) -> String {
        match issue {
            BgpIssueRef::NoAddress => "API 網域無對應協定棧位址".to_owned(),
            BgpIssueRef::TimedOut => "請求逾時".to_owned(),
            BgpIssueRef::TooManyRedirects => "HTTP 轉址次數過多".to_owned(),
            BgpIssueRef::InvalidRedirect(why) => format!("無效轉址：{why}"),
            BgpIssueRef::NotHttps(why) => format!("非 HTTPS 介面：{why}"),
            BgpIssueRef::Transport(why) => why.to_owned(),
            BgpIssueRef::HttpStatus(status) => format!("bgp-state 回傳 HTTP {status}"),
            BgpIssueRef::InvalidJson(why) => format!("bgp-state JSON 解析失敗：{why}"),
            BgpIssueRef::ApiStatus(status) => format!("bgp-state 狀態異常 `{status}`"),
            BgpIssueRef::MissingData => "bgp-state 回應缺少 `data` 欄位".to_owned(),
            BgpIssueRef::NoData => "未回傳 BGP 資料".to_owned(),
        }
    }

    fn bgp_upload_http_error(&self, status: u16, reason: &str) -> String {
        format!("HTTP {status}：{reason}")
    }

    fn bgp_upload_invalid_json(&self) -> String {
        "圖床回傳了無效的 JSON".to_owned()
    }

    fn bgp_upload_rejected(&self) -> String {
        "圖床拒絕了上傳請求".to_owned()
    }

    fn bgp_upload_missing_key(&self) -> String {
        "圖床回應缺少有效的 imageKey".to_owned()
    }

    fn bgp_upload_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} 拓撲圖上傳失敗：{reason}")
    }

    fn bgp_family_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} BGP 查詢失敗：{reason}")
    }

    fn bgp_query_failed(&self, reason: &str) -> String {
        format!("BGP 路由狀態查詢失敗：{reason}")
    }
}

impl TaierWords for TraditionalChinese {
    fn taier_method(&self, choice: &TaierChoice) -> String {
        let mut method = format!(
            "中國信通院全球網測（泰爾）：{} 並行串流，單向 {} 秒，HTTP/1.1",
            choice.streams, choice.seconds
        );
        if !choice.selections.is_empty() {
            const MAX_SHOWN: usize = 5;
            if choice.selections.len() <= MAX_SHOWN {
                method.push_str(&format!("；目標 {}", choice.selections.join(", ")));
            } else {
                let head = choice.selections[..MAX_SHOWN].join(", ");
                let rest = choice.selections.len() - MAX_SHOWN;
                method.push_str(&format!("；目標 {head}（另有 {rest} 項）"));
            }
        } else {
            if !choice.cities.is_empty() {
                method.push_str(&format!("；城市 {}", choice.cities.join(", ")));
            }
            if !choice.operators.is_empty() {
                method.push_str(&format!("；電信商 {}", choice.operators.join(", ")));
            }
        }
        if choice.custom_list {
            method.push_str("；使用自訂節點清單");
        }
        method
    }

    fn taier_unknown_cities(&self, names: &[String]) -> String {
        format!("泰爾測速節點清單中不存在：{}", names.join(", "))
    }

    fn taier_headers(&self) -> [&'static str; 4] {
        ["節點地區", "電信商", "下載速度", "上傳速度"]
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
            "telecom" => "電信",
            "unicom" => "聯通",
            "mobile" => "移動",
            "cernet" => "教育網",
            "cbn" => "廣電",
            _ => "其他",
        }
    }

    fn taier_cell(&self, cell: TaierCell) -> &'static str {
        match cell {
            TaierCell::Busy => "忙碌",
            TaierCell::Unreachable => "無法連線",
            TaierCell::AuthRejected => "驗證被拒",
            TaierCell::TimedOut => "逾時",
            TaierCell::Error => "錯誤",
        }
    }

    fn taier_fallback(&self, place: &str, operator: &str, tried: u8) -> String {
        format!(
            "{place}（{operator}）：前序節點忙碌或無法連線，已切換至第 {tried} 個備援節點完成測速"
        )
    }

    fn taier_node_failed(&self, place: &str, operator: &str, reason: &str) -> String {
        format!("{place}（{operator}）：失敗：{reason}")
    }

    fn taier_list_problem(&self, problem: TaierListProblem<'_>) -> String {
        match problem {
            TaierListProblem::Missing => "缺少泰爾測速節點清單".to_owned(),
            TaierListProblem::Unreadable(reason) => {
                format!("無法讀取泰爾測速節點清單：{reason}")
            }
            TaierListProblem::TooLarge(limit) => {
                format!("泰爾測速節點清單超過 {limit} 位元組上限")
            }
            TaierListProblem::Format(reason) => {
                format!("泰爾測速節點清單格式無效：{reason}")
            }
            TaierListProblem::Newer(version) => {
                format!("泰爾節點清單需要更新版本的 lowendaff-bench（格式版本 {version}）")
            }
            TaierListProblem::Version(version) => {
                format!("泰爾節點清單宣告了不存在的格式版本：{version}")
            }
            TaierListProblem::Empty => "泰爾測速節點清單為空".to_owned(),
            TaierListProblem::TooMany(count, most) => {
                format!("泰爾測速節點清單包含 {count} 個地區，超出上限 {most}")
            }
            TaierListProblem::Rule(position, rule) => {
                let detail = match rule {
                    TaierRule::Id => "id 必須為 1 至 40 個小寫字母、數字或 -（且不為 all）",
                    TaierRule::Province => "province 必須為 1 至 40 個可列印 ASCII 字元",
                    TaierRule::City => "city 必須為 1 至 40 個可列印 ASCII 字元",
                    TaierRule::Nodes => "nodes 必須包含 1 至 32 個節點項目",
                    TaierRule::Operator => {
                        "operator 必須為 telecom, unicom, mobile, cernet, cbn 或 other"
                    }
                    TaierRule::Ip => "ip 必須為規範格式的 IPv4 位址",
                    TaierRule::Port => "port 必須在 1024..=65535 範圍內",
                    TaierRule::DuplicateEndpoint => "同一地區內存在重複的 ip:port 節點",
                };
                format!("泰爾節點清單第 {position} 個地區違反規則：{detail}")
            }
            TaierListProblem::Duplicate(id) => {
                format!("泰爾節點清單存在重複的地區 ID：{id}")
            }
        }
    }

    fn taier_selector_problem(&self, problem: TaierSelectorProblem<'_>) -> String {
        match problem {
            TaierSelectorProblem::Empty => "空的泰爾目標選擇器".to_owned(),
            TaierSelectorProblem::Format(raw) => format!(
                "無效的泰爾選擇器 `{raw}`：格式應為 <地區>[:<電信商>[+<電信商>...]]\
                 （例如 beijing:cu 或 guangdong/shenzhen:ct+cm）"
            ),
            TaierSelectorProblem::EmptyPlace(raw) => {
                format!("無效的泰爾選擇器 `{raw}`：缺少地區名稱")
            }
            TaierSelectorProblem::EmptyOperators(raw) => format!(
                "無效的泰爾選擇器 `{raw}`：`:` 後缺少電信商代碼\
                 （ct, cu, cm, edu, cbn, other 或 all）"
            ),
            TaierSelectorProblem::UnknownOperator { selector, operator } => format!(
                "泰爾選擇器 `{selector}` 中存在未知電信商 `{operator}`：可選 \
                 telecom/ct, unicom/cu, mobile/cm, cernet/edu, cbn, other 或 all"
            ),
        }
    }

    fn taier_catalog_header(&self, regions: usize, locations: usize) -> String {
        format!("中國三網測速 (network.taier)：共 {regions} 個省級區域，{locations} 個城市節點")
    }

    fn taier_catalog_carriers_legend(&self) -> &'static str {
        "電信商簡寫：ct = 電信，cu = 聯通，cm = 移動，edu = 教育網，cbn = 廣電，other = 其他"
    }

    fn taier_region_kind(&self, code: &str) -> &'static str {
        match code {
            "municipality" => "直轄市",
            "province" => "省份/自治區",
            _ => "境外/其他",
        }
    }

    fn taier_catalog_overview_headers(&self) -> [&'static str; 5] {
        ["區域", "類型", "城市數", "電信商", "--taier 選擇器範例"]
    }

    fn taier_catalog_footer_hint(&self) -> &'static str {
        "使用 --taier-list <區域>（如 --taier-list guangdong）可檢視該區域下的城市清單。"
    }

    fn taier_catalog_region_heading(&self, name: &str, municipality: bool, count: usize) -> String {
        if municipality {
            format!("{name}（直轄市，{count} 個節點地區）：")
        } else {
            format!("{name}（{count} 個節點地區）：")
        }
    }

    fn taier_catalog_city_headers(&self) -> [&'static str; 3] {
        ["選擇器", "城市", "可用電信商（備援節點數）"]
    }

    fn taier_no_node(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "泰爾節點清單中沒有相符於所選條件的電信商節點".to_owned()
        } else {
            format!("泰爾節點清單中沒有相符於 {} 的節點", unknown.join(", "))
        }
    }

    fn taier_no_result(&self) -> String {
        "所有中國三網測速節點均未回傳有效速率".to_owned()
    }
}

impl CloudflareWords for TraditionalChinese {
    fn cloudflare_method(&self, choice: &CloudflareChoice) -> String {
        let mut method = format!(
            "Cloudflare 測速：{} 並行工作數，單向 {} 秒，HTTPS",
            choice.concurrency, choice.seconds
        );
        if choice.udp {
            method.push_str(&format!(" + STUN UDP（{} 個探測封包）", choice.udp_packets));
        } else {
            method.push_str("；略過 STUN UDP 探測");
        }
        if choice.custom_endpoints {
            method.push_str("；自訂介面位址");
        }
        method
    }

    fn cloudflare_headers(&self) -> [&'static str; 3] {
        ["指標", "IPv4", "IPv6"]
    }

    fn cloudflare_field(&self, field: CloudflareField) -> &'static str {
        match field {
            CloudflareField::Server => "邊緣節點",
            CloudflareField::ClientIp => "用戶端 IP",
            CloudflareField::ClientAsn => "用戶端 ASN",
            CloudflareField::IdleLatency => "閒置延遲",
            CloudflareField::Download => "下載吞吐",
            CloudflareField::DownloadLatency => "下載負載延遲",
            CloudflareField::Upload => "上傳吞吐",
            CloudflareField::UploadLatency => "上傳負載延遲",
            CloudflareField::UdpLoss => "UDP 封包遺失率",
            CloudflareField::Quality => "連線品質評級",
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
            text.push_str(&format!(", 抖動 {jit}"));
        }
        if loss_pct > 0.0 {
            text.push_str(&format!(", 遺失 {loss_pct:.1}%"));
        }
        text.push(')');
        text
    }

    fn cloudflare_speed_cell(&self, speed: &str, p50_mbps: f64, p95_mbps: f64) -> String {
        format!("{speed} (p50 {p50_mbps:.1}, p95 {p95_mbps:.1} Mbit/s)")
    }

    fn cloudflare_udp_quality(&self, code: &str) -> &'static str {
        match code {
            "excellent" => "極佳",
            "good" => "良好",
            "acceptable" => "尚可",
            "poor" => "較差",
            "bad" => "惡劣",
            _ => "未知",
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
            text.push_str(&format!(", 亂序 {reordered_pct:.1}%"));
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
            Some(ms) => format!("緩衝膨脹 {bloat_grade} (+{ms:.1} ms)"),
            None => format!("緩衝膨脹 {bloat_grade}"),
        };
        let stab = match stab_cv {
            Some(cv) => format!("穩定性 {stab_grade} (CV {cv:.1}%)"),
            None => format!("穩定性 {stab_grade}"),
        };
        format!("{bloat}，{stab}")
    }

    fn cloudflare_issue(&self, issue: CloudflareIssueRef<'_>) -> String {
        match issue {
            CloudflareIssueRef::EmptyDownload => "下載位元組數為 0".to_owned(),
            CloudflareIssueRef::EmptyUpload => "上傳位元組數為 0".to_owned(),
            CloudflareIssueRef::NoAddress => "無對應協定棧解析位址".to_owned(),
            CloudflareIssueRef::TimedOut => "請求逾時".to_owned(),
            CloudflareIssueRef::NoResolver => "無可用 DNS 解析器".to_owned(),
            CloudflareIssueRef::Transport(why) => why.to_owned(),
        }
    }

    fn cloudflare_udp_note(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} STUN UDP 探測不可用：{reason}")
    }

    fn cloudflare_family_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} Cloudflare 測速失敗：{reason}")
    }

    fn cloudflare_no_result(&self, reason: &str) -> String {
        if reason.is_empty() {
            "Cloudflare 測速未產生有效結果".to_owned()
        } else {
            format!("Cloudflare 測速失敗：{reason}")
        }
    }
}

impl InetspeedWords for TraditionalChinese {
    fn inetspeed_method(&self, choice: &InetspeedChoice) -> String {
        let rounds = if choice.threads <= 1 {
            "單執行緒".to_owned()
        } else {
            format!("單執行緒與 {} 執行緒", choice.threads)
        };
        let mut method = format!(
            "Apple CDN (iNetSpeed)：{rounds}，上限 {} / {} 秒，{} 次閒置取樣",
            choice.max, choice.timeout_secs, choice.latency_count
        );
        if choice.forced_endpoint {
            method.push_str("；固定指定節點");
        }
        if choice.custom_endpoints {
            method.push_str("；自訂介面位址");
        }
        method
    }

    fn inetspeed_endpoint_label(&self) -> &'static str {
        "優選邊緣節點"
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
            (Some(addr), Some(rtt)) => format!("{addr} ({host}，{rtt:.2} ms，來自 {src})"),
            (Some(addr), None) => format!("{addr} ({host}，來自 {src})"),
            (None, _) if host.is_empty() => format!("未綁定 IP ({src})"),
            (None, _) => format!("{host} (未綁定 IP，{src})"),
        }
    }

    fn inetspeed_source(&self, code: &str) -> &'static str {
        match code {
            "user" => "手動固定",
            "doh" => "雙 DoH 優選",
            "system_dns" => "系統 DNS",
            _ => "預設 DNS",
        }
    }

    fn inetspeed_idle_label(&self) -> &'static str {
        "閒置延遲"
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
            "中位 {median_ms:.2} ms（最小 {min_ms:.2} / 平均 {avg_ms:.2} / \
             最大 {max_ms:.2} ms，抖動 {jitter_ms:.2} ms）"
        )
    }

    fn inetspeed_total_label(&self) -> &'static str {
        "總傳輸流量"
    }

    fn inetspeed_headers(&self) -> [&'static str; 5] {
        ["測試輪次", "執行緒數", "傳輸速率", "資料量", "負載延遲"]
    }

    fn inetspeed_round_name(&self, download: bool, single: bool) -> &'static str {
        match (download, single) {
            (true, true) => "下載（單執行緒）",
            (true, false) => "下載（多執行緒）",
            (false, true) => "上傳（單執行緒）",
            (false, false) => "上傳（多執行緒）",
        }
    }

    fn inetspeed_loaded_cell(&self, median_ms: f64, jitter_ms: f64) -> String {
        format!("{median_ms:.2} ms（抖動 {jitter_ms:.2} ms）")
    }

    fn inetspeed_round_issue(&self, issue: InetspeedRoundIssueRef<'_>) -> String {
        match issue {
            InetspeedRoundIssueRef::ZeroBytes => "傳輸完成但位元組數為 0".to_owned(),
            InetspeedRoundIssueRef::WorkerFaults(count) => {
                format!("{count} 個工作執行緒發生傳輸錯誤")
            }
            InetspeedRoundIssueRef::Transport(fault) => inetspeed_fault_text(fault),
        }
    }

    fn inetspeed_warning(&self, warning: InetspeedWarningRef<'_>) -> String {
        match warning {
            InetspeedWarningRef::HostMismatch => {
                "下載、上傳與延遲探測網域不一致，已停用共用節點 IP 綁定".to_owned()
            }
            InetspeedWarningRef::ForcedProbeFailed(ip) => {
                format!("指定節點 {ip} 預探測失敗，繼續按該 IP 嘗試測速")
            }
            InetspeedWarningRef::DohEmpty => "雙 DoH 未回傳候選 IP，已回退至系統 DNS".to_owned(),
            InetspeedWarningRef::UnpinnedFallback => {
                "節點探索未取得可用 IP，繼續使用非綁定系統 DNS".to_owned()
            }
            InetspeedWarningRef::NoIdleSamples => "未擷取到有效的閒置延遲樣本".to_owned(),
            InetspeedWarningRef::IdleFailed(fault) => {
                format!("閒置延遲探測失敗：{}", inetspeed_fault_text(fault))
            }
        }
    }

    fn inetspeed_invalid_max(&self, raw: &str) -> String {
        format!("無效的流量上限 {raw:?}：應為如 2G、500M 或 1GiB 的容量格式")
    }

    fn inetspeed_no_result(&self) -> String {
        "Apple CDN 測速未能完成任何傳輸輪次".to_owned()
    }
}

fn inetspeed_fault_text(fault: InetspeedProbeFaultRef<'_>) -> String {
    match fault {
        InetspeedProbeFaultRef::EmptySample => "探測未回傳樣本".to_owned(),
        InetspeedProbeFaultRef::NoAddress => "無對應協定棧位址".to_owned(),
        InetspeedProbeFaultRef::TimedOut => "請求逾時".to_owned(),
        InetspeedProbeFaultRef::NoResolver => "無可用 DNS 解析器".to_owned(),
        InetspeedProbeFaultRef::Transport(why) => why.to_owned(),
    }
}
