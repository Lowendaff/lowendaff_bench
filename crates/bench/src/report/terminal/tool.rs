//! The words for a run of a tool that went wrong, which the groups share.

use bench_tool::job;

use crate::locale;
use crate::locale::Unit;

/// How a run of `tool` went wrong; `unit` tells its time limit.
pub(super) fn fault(tool: &str, fault: &job::Fault, unit: Unit) -> String {
    locale::words().tool_fault(tool, fault, unit)
}
