//! English: the progress log's lines.

use bench_api::ToolEnd;

use super::English;
use crate::locale::LogWords;

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

    fn item_progress(&self, id: &str, line: &str) -> String {
        format!("{id} {line}")
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
