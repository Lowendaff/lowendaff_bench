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

    fn item_progress(&self, id: &str, step: Step) -> String {
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
            Step::NodeBandwidth { cpus, memory } => self.numa_run(cpus, memory, Probe::Bandwidth),
            Step::NodeLatency { cpus, memory } => self.numa_run(cpus, memory, Probe::Latency),
            Step::RunAgain { run } => format!("the upload failed; run {run}"),
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

    fn interrupt(&self) -> String {
        "Interrupt received: stopping the current test; press Ctrl-C again to exit at once"
            .to_owned()
    }
}
