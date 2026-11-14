//! What multichase prints for one level: the latency, or why there is none.

use bench_tool::job;

use super::result::Fault;
use crate::Value;
use crate::chase;

/// The latency of an attempt in nanoseconds, or why there is none.
pub fn latency(output: &job::Output) -> Result<Value, Fault> {
    if let Some(fault) = output.fault() {
        return Err(Fault::Tool(fault));
    }
    chase::latency(output).ok_or(Fault::NoLatency)
}
