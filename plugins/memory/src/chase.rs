//! multichase, which the latency and the NUMA plugins both run: how long it samples, how long it
//! may take, and the one number it prints.

use std::time::Duration;

use bench_tool::job;

use crate::Value;

/// The half-second samples of each multichase run: 2.5 seconds.
pub const SAMPLES: u32 = 5;

/// A multichase or multiload run samples for a few seconds after filling its buffer.
pub const RUN_LIMIT: Duration = Duration::from_secs(60);

/// multichase's best latency in nanoseconds: the one number it prints, on its last line.
#[must_use]
pub fn latency(output: &job::Output) -> Option<Value> {
    output.last_line().and_then(Value::parse)
}
