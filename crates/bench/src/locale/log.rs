//! The progress log's lines, on stderr; the lines that only `-v` shows say so.

use bench_api::ToolEnd;

/// The progress log's lines.
pub trait LogWords {
    /// Where the program runs: the platform's name and the architecture.
    fn host_line(&self, platform: &str, arch: &str) -> String;
    /// An address family, such as IPv4, and its reachability and public address in the report's
    /// words.
    fn network_line(&self, family: &str, text: &str) -> String;
    /// `-v`: a source that gave the family's public address, which comes masked.
    fn network_source(&self, source: &str, family: &str, address: &str) -> String;
    /// `-v`: a source that gave no address, with its reason in the report's words.
    fn network_note(&self, text: &str) -> String;
    /// `-v`: the download of one file.
    fn download(&self, url: &str) -> String;
    /// The tools that downloaded and passed their checks, each with its version.
    fn tools_ready(&self, tools: &[String]) -> String;
    /// A tool that could not be provisioned, and why.
    fn tool_failed(&self, tool: &str, version: &str, reason: &str) -> String;
    /// A test item that starts.
    fn item_started(&self, id: &str) -> String;
    /// A progress line of a test item.
    fn item_progress(&self, id: &str, line: &str) -> String;
    /// How a test item ended, in the report's words.
    fn item_ended(&self, id: &str, status: &str) -> String;
    /// `-v`: a tool that a test item ran, its command, and how it ended.
    fn tool_run(&self, id: &str, command: &str, end: &ToolEnd) -> String;
    /// `-v`: one of the last lines that a tool printed.
    fn tool_output(&self, id: &str, line: &str) -> String;
    /// The first Ctrl-C: what happens now, and what a second one does.
    fn interrupt(&self) -> String;
}
