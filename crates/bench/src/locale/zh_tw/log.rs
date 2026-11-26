//! 繁體中文：即時進度日誌文字。

use bench_api::Step;
use bench_api::ToolEnd;

use super::TraditionalChinese;
use crate::locale::LogWords;
use crate::locale::Probe;

impl LogWords for TraditionalChinese {
    fn host_line(&self, platform: &str, arch: &str) -> String {
        format!("主機：{platform}，{arch}")
    }

    fn network_line(&self, family: &str, text: &str) -> String {
        format!("網路：{family} {text}")
    }

    fn network_source(&self, source: &str, family: &str, address: &str) -> String {
        format!("網路：{source} 回傳 {family} {address}")
    }

    fn network_note(&self, text: &str) -> String {
        format!("網路：{text}")
    }

    fn download(&self, url: &str) -> String {
        format!("正在下載 {url}")
    }

    fn tools_ready(&self, tools: &[String]) -> String {
        format!("工具就緒：{}", tools.join("、"))
    }

    fn tool_failed(&self, tool: &str, version: &str, reason: &str) -> String {
        format!("工具失敗：{tool} {version}：{reason}")
    }

    fn item_started(&self, id: &str) -> String {
        format!("{id} 開始執行")
    }

    fn item_progress(&self, id: &str, step: &Step) -> String {
        use crate::locale::CpuWords;
        use crate::locale::IpWords;
        use crate::locale::MediaWords;
        use crate::locale::MemoryWords;

        let step = match step {
            Step::Test { threads: 1 } => "單執行緒測試".to_owned(),
            Step::Test { threads } => format!("多執行緒測試（{threads} 執行緒）"),
            Step::Read { threads: 1 } => "單執行緒讀取".to_owned(),
            Step::Read { threads } => format!("多執行緒讀取（{threads} 執行緒）"),
            Step::Write { threads: 1 } => "單執行緒寫入".to_owned(),
            Step::Write { threads } => format!("多執行緒寫入（{threads} 執行緒）"),
            Step::TestFile => "正在建立測試檔案".to_owned(),
            Step::RandomReadWrite { block_size } => {
                format!("{} 混合隨機讀寫測試", block_size.to_uppercase())
            }
            Step::Cache { level } => format!("L{level} 快取測試"),
            Step::MainMemory => "主記憶體測試".to_owned(),
            Step::NodeBandwidth { cpus, memory } => self.numa_run(*cpus, *memory, Probe::Bandwidth),
            Step::NodeLatency { cpus, memory } => self.numa_run(*cpus, *memory, Probe::Latency),
            Step::RunAgain { run } => format!("結果上傳失敗，正在重試第 {run} 輪"),
            Step::Upload {
                location,
                country,
                ipv6,
            } => format!("上傳測速：{location}, {country}（{}）", family(*ipv6)),
            Step::Download {
                location,
                country,
                ipv6,
            } => format!("下載測速：{location}, {country}（{}）", family(*ipv6)),
            Step::Trace { target, ipv6 } => {
                format!("路由追蹤：{target}（{}）", family(*ipv6))
            }
            Step::Media { group, ipv6 } => {
                format!(
                    "串流媒體檢測：{}（{}）",
                    self.media_group(group),
                    family(*ipv6)
                )
            }
            Step::IpQuality { stage, ipv6 } => {
                format!("正在檢測 {}（{}）", self.ip_stage(stage), family(*ipv6))
            }
            Step::Bgp { ipv6 } => format!("正在查詢 {} BGP 路由表", family(*ipv6)),
            Step::CpuSuite {
                suite,
                stage,
                threads,
            } => self.cpu_suite_step(suite, *stage, *threads),
            Step::Taier {
                location,
                operator,
                stage,
            } => format!("{location}（{operator}）：{stage}"),
            Step::Cloudflare { stage, ipv6 } => {
                let phase = match *stage {
                    "latency" => "閒置延遲探測",
                    "download" => "下載吞吐與負載延遲",
                    "upload" => "上傳吞吐與負載延遲",
                    "udp" => "STUN UDP 封包遺失探測",
                    other => other,
                };
                format!("{phase}（{}）", family(*ipv6))
            }
            Step::Inetspeed { stage, threads } => match (*stage, *threads) {
                ("discover", _) => "正在優選最佳 Apple CDN 邊緣節點".to_owned(),
                ("latency", _) => "正在測量閒置延遲".to_owned(),
                (dir, 1) => format!("{dir}（單執行緒）"),
                (dir, n) => format!("{dir}（{n} 執行緒）"),
            },
            Step::Tcping {
                group,
                targets,
                ipv6,
            } => {
                let label = match *group {
                    "china" => "中國大陸各省三網",
                    "china_premium" => "中國大陸精品網",
                    "global" => "全球核心樞紐",
                    other => other,
                };
                format!("正在探測 {label}（{targets} 個節點，{}）", family(*ipv6))
            }
        };
        format!("{id} {step}")
    }

    fn item_ended(&self, id: &str, status: &str) -> String {
        format!("{id} {status}")
    }

    fn tool_run(&self, id: &str, command: &str, end: &ToolEnd) -> String {
        let end = match end {
            ToolEnd::Exit(code) => format!("結束碼 {code}"),
            ToolEnd::Signal => "被訊號終止".to_owned(),
            ToolEnd::TimedOut(limit) => {
                format!("執行超過 {} 秒仍未結束，已終止", limit.as_secs())
            }
            ToolEnd::Cancelled => "已中斷".to_owned(),
            ToolEnd::Spawn(error) => format!("無法啟動：{error}"),
            ToolEnd::Read(error) => format!("無法讀取輸出：{error}"),
        };
        format!("{id} 執行 {command}：{end}")
    }

    fn tool_output(&self, id: &str, line: &str) -> String {
        format!("{id} 輸出：{line}")
    }

    fn http_probe(&self, method: &str, host: &str, ipv6: bool, outcome: &str) -> String {
        format!("探測 {method} {host}（{}）：{outcome}", family(ipv6))
    }

    fn smtp_probe(&self, host: &str, port: u16, ipv6: bool, outcome: &str) -> String {
        format!("探測 SMTP {host}:{port}（{}）：{outcome}", family(ipv6))
    }

    fn interrupt(&self) -> String {
        "收到中斷訊號：正在停止目前測試項目；再次按 Ctrl-C 將立即離開".to_owned()
    }
}

fn family(ipv6: bool) -> &'static str {
    if ipv6 { "IPv6" } else { "IPv4" }
}
