//! 简体中文：实时进度日志文本。

use bench_api::Step;
use bench_api::ToolEnd;

use super::SimplifiedChinese;
use crate::locale::LogWords;
use crate::locale::Probe;

impl LogWords for SimplifiedChinese {
    fn host_line(&self, platform: &str, arch: &str) -> String {
        format!("主机：{platform}，{arch}")
    }

    fn network_line(&self, family: &str, text: &str) -> String {
        format!("网络：{family} {text}")
    }

    fn network_source(&self, source: &str, family: &str, address: &str) -> String {
        format!("网络：{source} 返回 {family} {address}")
    }

    fn network_note(&self, text: &str) -> String {
        format!("网络：{text}")
    }

    fn download(&self, url: &str) -> String {
        format!("正在下载 {url}")
    }

    fn tools_ready(&self, tools: &[String]) -> String {
        format!("工具就绪：{}", tools.join("、"))
    }

    fn tool_failed(&self, tool: &str, version: &str, reason: &str) -> String {
        format!("工具失败：{tool} {version}：{reason}")
    }

    fn item_started(&self, id: &str) -> String {
        format!("{id} 开始运行")
    }

    fn item_progress(&self, id: &str, step: &Step) -> String {
        use crate::locale::CpuWords;
        use crate::locale::IpWords;
        use crate::locale::MediaWords;
        use crate::locale::MemoryWords;

        let step = match step {
            Step::Test { threads: 1 } => "单线程测试".to_owned(),
            Step::Test { threads } => format!("多线程测试（{threads} 线程）"),
            Step::Read { threads: 1 } => "单线程读取".to_owned(),
            Step::Read { threads } => format!("多线程读取（{threads} 线程）"),
            Step::Write { threads: 1 } => "单线程写入".to_owned(),
            Step::Write { threads } => format!("多线程写入（{threads} 线程）"),
            Step::TestFile => "正在创建测试文件".to_owned(),
            Step::RandomReadWrite { block_size } => {
                format!("{} 混合随机读写测试", block_size.to_uppercase())
            }
            Step::Cache { level } => format!("L{level} 缓存测试"),
            Step::MainMemory => "主内存测试".to_owned(),
            Step::NodeBandwidth { cpus, memory } => self.numa_run(*cpus, *memory, Probe::Bandwidth),
            Step::NodeLatency { cpus, memory } => self.numa_run(*cpus, *memory, Probe::Latency),
            Step::RunAgain { run } => format!("结果上传失败，正在重试第 {run} 轮"),
            Step::Upload {
                location,
                country,
                ipv6,
            } => format!("上传测速：{location}, {country}（{}）", family(*ipv6)),
            Step::Download {
                location,
                country,
                ipv6,
            } => format!("下载测速：{location}, {country}（{}）", family(*ipv6)),
            Step::Trace { target, ipv6 } => {
                format!("路由追踪：{target}（{}）", family(*ipv6))
            }
            Step::Media { group, ipv6 } => {
                format!(
                    "流媒体检测：{}（{}）",
                    self.media_group(group),
                    family(*ipv6)
                )
            }
            Step::IpQuality { stage, ipv6 } => {
                format!("正在检测 {}（{}）", self.ip_stage(stage), family(*ipv6))
            }
            Step::Bgp { ipv6 } => format!("正在查询 {} BGP 路由表", family(*ipv6)),
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
                    "latency" => "空载延迟探测",
                    "download" => "下载吞吐与负载延迟",
                    "upload" => "上传吞吐与负载延迟",
                    "udp" => "STUN UDP 丢包探测",
                    other => other,
                };
                format!("{phase}（{}）", family(*ipv6))
            }
            Step::Inetspeed { stage, threads } => match (*stage, *threads) {
                ("discover", _) => "正在优选最佳 Apple CDN 边缘节点".to_owned(),
                ("latency", _) => "正在测量空载延迟".to_owned(),
                (dir, 1) => format!("{dir}（单线程）"),
                (dir, n) => format!("{dir}（{n} 线程）"),
            },
            Step::Tcping {
                group,
                targets,
                ipv6,
            } => {
                let label = match *group {
                    "china" => "中国大陆各省三网",
                    "china_premium" => "中国大陆精品网",
                    "global" => "全球核心枢纽",
                    other => other,
                };
                format!("正在探测 {label}（{targets} 个节点，{}）", family(*ipv6))
            }
        };
        format!("{id} {step}")
    }

    fn item_ended(&self, id: &str, status: &str) -> String {
        format!("{id} {status}")
    }

    fn tool_run(&self, id: &str, command: &str, end: &ToolEnd) -> String {
        let end = match end {
            ToolEnd::Exit(code) => format!("退出码 {code}"),
            ToolEnd::Signal => "被信号终止".to_owned(),
            ToolEnd::TimedOut(limit) => {
                format!("运行超过 {} 秒仍未结束，已终止", limit.as_secs())
            }
            ToolEnd::Cancelled => "已中断".to_owned(),
            ToolEnd::Spawn(error) => format!("无法启动：{error}"),
            ToolEnd::Read(error) => format!("无法读取输出：{error}"),
        };
        format!("{id} 执行 {command}：{end}")
    }

    fn tool_output(&self, id: &str, line: &str) -> String {
        format!("{id} 输出：{line}")
    }

    fn http_probe(&self, method: &str, host: &str, ipv6: bool, outcome: &str) -> String {
        format!("探测 {method} {host}（{}）：{outcome}", family(ipv6))
    }

    fn smtp_probe(&self, host: &str, port: u16, ipv6: bool, outcome: &str) -> String {
        format!("探测 SMTP {host}:{port}（{}）：{outcome}", family(ipv6))
    }

    fn interrupt(&self) -> String {
        "收到中断信号：正在停止当前测试项；再次按 Ctrl-C 将立即退出".to_owned()
    }
}

fn family(ipv6: bool) -> &'static str {
    if ipv6 { "IPv6" } else { "IPv4" }
}
