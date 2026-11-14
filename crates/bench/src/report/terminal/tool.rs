//! The words for a run of a tool that went wrong, which the groups share.

use std::time::Duration;

use bench_tool::job;

/// How a run of `tool` went wrong; `limit` words its time limit.
pub(super) fn fault(tool: &str, fault: &job::Fault, limit: fn(Duration) -> String) -> String {
    let quoted = |last: &Option<String>| last.clone().unwrap_or_else(|| "没有输出".to_owned());
    match fault {
        job::Fault::Spawn(error) => format!("无法运行 {tool}：{error}"),
        job::Fault::Read(error) => format!("无法读取 {tool} 的输出：{error}"),
        job::Fault::TimedOut(duration) => format!("超过 {}仍未结束，已停止", limit(*duration)),
        job::Fault::Exit { code, last } => format!("{tool} 退出码 {code}：{}", quoted(last)),
        job::Fault::Signal { last } => format!("{tool} 被信号终止：{}", quoted(last)),
    }
}

/// A limit in seconds, such as `60 秒`.
pub(super) fn seconds(limit: Duration) -> String {
    format!("{} 秒", limit.as_secs())
}

/// A limit in minutes, such as `60 分钟`.
pub(super) fn minutes(limit: Duration) -> String {
    format!("{} 分钟", limit.as_secs() / 60)
}
