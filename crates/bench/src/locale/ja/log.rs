//! 日本語：リアルタイム進捗ログの文言。

use bench_api::Step;
use bench_api::ToolEnd;

use super::Japanese;
use crate::locale::LogWords;
use crate::locale::Probe;

impl LogWords for Japanese {
    fn host_line(&self, platform: &str, arch: &str) -> String {
        format!("ホスト：{platform}、{arch}")
    }

    fn network_line(&self, family: &str, text: &str) -> String {
        format!("ネットワーク：{family} {text}")
    }

    fn network_source(&self, source: &str, family: &str, address: &str) -> String {
        format!("ネットワーク：{source} が {family} {address} を返却")
    }

    fn network_note(&self, text: &str) -> String {
        format!("ネットワーク：{text}")
    }

    fn download(&self, url: &str) -> String {
        format!("ダウンロード中：{url}")
    }

    fn tools_ready(&self, tools: &[String]) -> String {
        format!("ツール準備完了：{}", tools.join("、"))
    }

    fn tool_failed(&self, tool: &str, version: &str, reason: &str) -> String {
        format!("ツール準備失敗：{tool} {version}：{reason}")
    }

    fn item_started(&self, id: &str) -> String {
        format!("{id} 開始")
    }

    fn item_progress(&self, id: &str, step: &Step) -> String {
        use crate::locale::CpuWords;
        use crate::locale::IpWords;
        use crate::locale::MediaWords;
        use crate::locale::MemoryWords;

        let step = match step {
            Step::Test { threads: 1 } => "シングルスレッド測定".to_owned(),
            Step::Test { threads } => format!("マルチスレッド測定（{threads} スレッド）"),
            Step::Read { threads: 1 } => "シングルスレッド読込".to_owned(),
            Step::Read { threads } => format!("マルチスレッド読込（{threads} スレッド）"),
            Step::Write { threads: 1 } => "シングルスレッド書込".to_owned(),
            Step::Write { threads } => format!("マルチスレッド書込（{threads} スレッド）"),
            Step::TestFile => "テストファイルを作成中".to_owned(),
            Step::RandomReadWrite { block_size } => {
                format!("{} ランダム読み書き測定", block_size.to_uppercase())
            }
            Step::Cache { level } => format!("L{level} キャッシュ測定"),
            Step::MainMemory => "メインメモリ測定".to_owned(),
            Step::NodeBandwidth { cpus, memory } => self.numa_run(*cpus, *memory, Probe::Bandwidth),
            Step::NodeLatency { cpus, memory } => self.numa_run(*cpus, *memory, Probe::Latency),
            Step::RunAgain { run } => {
                format!("アップロード失敗のため再実行（{run} 回目）")
            }
            Step::Upload {
                location,
                country,
                ipv6,
            } => format!("上り測定：{location}, {country}（{}）", family(*ipv6)),
            Step::Download {
                location,
                country,
                ipv6,
            } => format!("下り測定：{location}, {country}（{}）", family(*ipv6)),
            Step::Trace { target, ipv6 } => {
                format!("経路追跡：{target}（{}）", family(*ipv6))
            }
            Step::Media { group, ipv6 } => {
                format!("配信判定：{}（{}）", self.media_group(group), family(*ipv6))
            }
            Step::IpQuality { stage, ipv6 } => {
                format!("{} を確認中（{}）", self.ip_stage(stage), family(*ipv6))
            }
            Step::Bgp { ipv6 } => format!("{} BGP 経路を照会中", family(*ipv6)),
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
                    "latency" => "無負荷遅延の測定",
                    "download" => "下り速度・高負荷時遅延",
                    "upload" => "上り速度・高負荷時遅延",
                    "udp" => "STUN UDP パケットロス調査",
                    other => other,
                };
                format!("{phase}（{}）", family(*ipv6))
            }
            Step::Inetspeed { stage, threads } => match (*stage, *threads) {
                ("discover", _) => "最適な Apple CDN エッジ IP を探索中".to_owned(),
                ("latency", _) => "無負荷遅延を測定中".to_owned(),
                (dir, 1) => format!("{dir}（シングルスレッド）"),
                (dir, n) => format!("{dir}（{n} スレッド）"),
            },
            Step::Tcping {
                group,
                targets,
                ipv6,
            } => {
                let label = match *group {
                    "china" => "中国本土キャリア",
                    "china_premium" => "中国プレミアム回線",
                    "global" => "グローバル主要拠点",
                    other => other,
                };
                format!("{label} を測定中（{targets} 拠点、{}）", family(*ipv6))
            }
        };
        format!("{id} {step}")
    }

    fn item_ended(&self, id: &str, status: &str) -> String {
        format!("{id} {status}")
    }

    fn tool_run(&self, id: &str, command: &str, end: &ToolEnd) -> String {
        let end = match end {
            ToolEnd::Exit(code) => format!("終了コード {code}"),
            ToolEnd::Signal => "シグナルにより終了".to_owned(),
            ToolEnd::TimedOut(limit) => {
                format!("{} 秒経過しても終了しないため停止", limit.as_secs())
            }
            ToolEnd::Cancelled => "中断".to_owned(),
            ToolEnd::Spawn(error) => format!("起動失敗：{error}"),
            ToolEnd::Read(error) => format!("出力の読み取り失敗：{error}"),
        };
        format!("{id} 実行 {command}：{end}")
    }

    fn tool_output(&self, id: &str, line: &str) -> String {
        format!("{id} 出力：{line}")
    }

    fn http_probe(&self, method: &str, host: &str, ipv6: bool, outcome: &str) -> String {
        format!("プローブ {method} {host}（{}）：{outcome}", family(ipv6))
    }

    fn smtp_probe(&self, host: &str, port: u16, ipv6: bool, outcome: &str) -> String {
        format!("プローブ SMTP {host}:{port}（{}）：{outcome}", family(ipv6))
    }

    fn interrupt(&self) -> String {
        "割り込みを受信：現在のテストを停止しています。もう一度 Ctrl-C を押すと即座に終了します"
            .to_owned()
    }
}

fn family(ipv6: bool) -> &'static str {
    if ipv6 { "IPv6" } else { "IPv4" }
}
