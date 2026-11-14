//! What multiload and multichase print for one cell: the bandwidth or the latency, a refused
//! binding of memory to a node, or why there is no value.

use bench_tool::job;

use super::result::Fault;
use crate::Value;
use crate::chase;

/// multiload's best bandwidth in MiB/s: the `LdMaxMibs` column of the row under its header.
pub fn bandwidth(output: &job::Output) -> Result<Value, Fault> {
    finished(output)?;
    let mut lines = output.text.lines();
    lines
        .by_ref()
        .map(fields)
        .find(|header| header.contains(&"LdMaxMibs"))
        .and_then(|header| {
            let column = header.iter().position(|field| *field == "LdMaxMibs")?;
            let row = lines.map(fields).rfind(|row| row.len() == header.len())?;
            Value::parse(row[column])
        })
        .ok_or(Fault::NoBandwidth)
}

/// multichase's latency in nanoseconds.
pub fn latency(output: &job::Output) -> Result<Value, Fault> {
    finished(output)?;
    chase::latency(output).ok_or(Fault::NoLatency)
}

/// Why a run ended without a value: a refused binding, which the tools report through
/// `perror` and an exit code of 1, or another exit code than 0.
fn finished(output: &job::Output) -> Result<(), Fault> {
    let refused = output
        .text
        .lines()
        .any(|line| line.trim() == "mbind: Operation not permitted");
    if refused {
        return Err(Fault::Refused);
    }
    output
        .fault()
        .map_or(Ok(()), |fault| Err(Fault::Tool(fault)))
}

/// The comma-separated fields of a line of multiload's output, without padding.
fn fields(line: &str) -> Vec<&str> {
    line.split(',').map(str::trim).collect()
}
