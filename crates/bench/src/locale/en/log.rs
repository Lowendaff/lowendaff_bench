//! English: the progress log's lines.

use bench_api::Step;
use bench_api::ToolEnd;

use super::English;
use crate::locale::LogWords;
use crate::locale::Probe;

impl LogWords for English {
    fn host_line(&self, platform: &str, arch: &str) -> String {
        format!("Host: {platform}, {arch}")
    }

    fn network_line(&self, family: &str, text: &str) -> String {
        format!("Network: {family} {text}")
    }

    fn network_source(&self, source: &str, family: &str, address: &str) -> String {
        format!("Network: {source} returned {family} {address}")
    }

    fn network_note(&self, text: &str) -> String {
        format!("Network: {text}")
    }

    fn download(&self, url: &str) -> String {
        format!("Downloading {url}")
    }

    fn tools_ready(&self, tools: &[String]) -> String {
        format!("Tools: {} ready", tools.join(", "))
    }

    fn tool_failed(&self, tool: &str, version: &str, reason: &str) -> String {
        format!("Tools: {tool} {version} failed: {reason}")
    }

    fn item_started(&self, id: &str) -> String {
        format!("{id} started")
    }

    fn item_progress(&self, id: &str, step: &Step) -> String {
        use crate::locale::CpuWords;
        use crate::locale::IpWords;
        use crate::locale::MediaWords;
        use crate::locale::MemoryWords;

        let step = match step {
            Step::Test { threads: 1 } => "single-thread test".to_owned(),
            Step::Test { threads } => format!("multi-thread test ({threads} threads)"),
            Step::Read { threads: 1 } => "single-thread read".to_owned(),
            Step::Read { threads } => format!("multi-thread read ({threads} threads)"),
            Step::Write { threads: 1 } => "single-thread write".to_owned(),
            Step::Write { threads } => format!("multi-thread write ({threads} threads)"),
            Step::TestFile => "creating the test file".to_owned(),
            Step::RandomReadWrite { block_size } => {
                format!("{} random read and write test", block_size.to_uppercase())
            }
            Step::Cache { level } => format!("L{level} test"),
            Step::MainMemory => "memory test".to_owned(),
            Step::NodeBandwidth { cpus, memory } => self.numa_run(*cpus, *memory, Probe::Bandwidth),
            Step::NodeLatency { cpus, memory } => self.numa_run(*cpus, *memory, Probe::Latency),
            Step::RunAgain { run } => format!("the upload failed; run {run}"),
            Step::Upload {
                location,
                country,
                ipv6,
            } => format!("upload to {location}, {country} over {}", family(*ipv6)),
            Step::Download {
                location,
                country,
                ipv6,
            } => format!("download from {location}, {country} over {}", family(*ipv6)),
            Step::Trace { target, ipv6 } => format!("trace to {target} over {}", family(*ipv6)),
            Step::Media { group, ipv6 } => {
                format!("probing {} over {}", self.media_group(group), family(*ipv6))
            }
            Step::IpQuality { stage, ipv6 } => {
                format!("checking {} for {}", self.ip_stage(stage), family(*ipv6))
            }
            Step::Bgp { ipv6 } => format!("querying BGP routes for {}", family(*ipv6)),
            Step::CpuSuite {
                suite,
                stage,
                threads,
            } => self.cpu_suite_step(suite, *stage, *threads),
            Step::Taier {
                location,
                operator,
                stage,
            } => format!("{location} ({operator}): {stage}"),
            Step::Cloudflare { stage, ipv6 } => {
                let phase = match *stage {
                    "latency" => "idle latency",
                    "download" => "download + loaded latency",
                    "upload" => "upload + loaded latency",
                    "udp" => "STUN UDP packet-loss probe",
                    other => other,
                };
                format!("{phase} over {}", family(*ipv6))
            }
            Step::Inetspeed { stage, threads } => match (*stage, *threads) {
                ("discover", _) => "discovering optimal Apple CDN endpoint".to_owned(),
                ("latency", _) => "measuring idle latency".to_owned(),
                (dir, 1) => format!("{dir} (single-thread)"),
                (dir, n) => format!("{dir} ({n} threads)"),
            },
        };
        format!("{id} {step}")
    }

    fn item_ended(&self, id: &str, status: &str) -> String {
        format!("{id} {status}")
    }

    fn tool_run(&self, id: &str, command: &str, end: &ToolEnd) -> String {
        let end = match end {
            ToolEnd::Exit(code) => format!("exit code {code}"),
            ToolEnd::Signal => "ended by a signal".to_owned(),
            ToolEnd::TimedOut(limit) => {
                format!("still running after {} s, stopped", limit.as_secs())
            }
            ToolEnd::Cancelled => "interrupted".to_owned(),
            ToolEnd::Spawn(error) => format!("cannot run: {error}"),
            ToolEnd::Read(error) => format!("cannot read its output: {error}"),
        };
        format!("{id} ran {command}: {end}")
    }

    fn tool_output(&self, id: &str, line: &str) -> String {
        format!("{id} output: {line}")
    }

    fn http_probe(&self, method: &str, host: &str, ipv6: bool, outcome: &str) -> String {
        format!("Probe {method} {host} over {}: {outcome}", family(ipv6))
    }

    fn smtp_probe(&self, host: &str, port: u16, ipv6: bool, outcome: &str) -> String {
        format!("Probe SMTP {host}:{port} over {}: {outcome}", family(ipv6))
    }

    fn interrupt(&self) -> String {
        "Interrupt received: stopping the current test; press Ctrl-C again to exit at once"
            .to_owned()
    }
}

/// The name of a network family: IPv6's, or else IPv4's.
fn family(ipv6: bool) -> &'static str {
    if ipv6 { "IPv6" } else { "IPv4" }
}
