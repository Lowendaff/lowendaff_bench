//! What each tool printed: its result, or why there is none.

/// A finished run of a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The exit code, or `None` after a signal.
    pub code: Option<i32>,
    /// Its standard output followed by its standard error.
    pub text: String,
}

/// A number as the tool printed it, and its value.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    /// The number as printed.
    pub text: String,
    /// The same number.
    pub value: f64,
}

/// Why an attempt ended without a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptError {
    /// The attempt failed for this reason; it may be retried.
    Failed(String),
    /// The kernel would not bind memory to a node, as in Docker's default containers; retrying
    /// cannot help.
    Refused,
    /// The run was cancelled; nothing is retried.
    Cancelled,
}

/// STREAM's kernels, in the order it prints them.
pub const KERNELS: [&str; 4] = ["Copy", "Scale", "Add", "Triad"];

/// sysbench's rate, in MiB/s, from `324864.00 MiB transferred (64956.37 MiB/sec)`.
pub fn sysbench(output: &Output) -> Result<Value, AttemptError> {
    finished("sysbench", output)?;
    output
        .text
        .lines()
        .find_map(|line| {
            let (_, rate) = line.trim().split_once("MiB transferred (")?;
            value(rate.strip_suffix("MiB/sec)")?)
        })
        .ok_or_else(|| AttemptError::Failed("sysbench 没有输出传输速度".to_owned()))
}

/// STREAM's best rate of each kernel, in MB/s, when its results validated.
pub fn stream(output: &Output) -> Result<[Value; 4], AttemptError> {
    finished("STREAM", output)?;
    let lines: Vec<&str> = output.text.lines().map(str::trim).collect();
    if !lines
        .iter()
        .any(|line| line.starts_with("Solution Validates"))
    {
        let reason = match lines
            .iter()
            .find(|line| line.starts_with("Failed Validation"))
        {
            Some(line) => format!("STREAM 的结果没有通过校验：{line}"),
            None => "STREAM 没有输出结果".to_owned(),
        };
        return Err(AttemptError::Failed(reason));
    }
    let rate = |kernel: &str| {
        lines.iter().find_map(|line| {
            let rest = line.strip_prefix(kernel)?.strip_prefix(':')?;
            value(rest.split_whitespace().next()?)
        })
    };
    let [Some(copy), Some(scale), Some(add), Some(triad)] = KERNELS.map(rate) else {
        return Err(AttemptError::Failed("STREAM 没有输出结果".to_owned()));
    };
    Ok([copy, scale, add, triad])
}

/// multichase's best latency, in nanoseconds: the one number it prints.
pub fn multichase(output: &Output) -> Result<Value, AttemptError> {
    refused(output)?;
    finished("multichase", output)?;
    value(last_line(&output.text))
        .ok_or_else(|| AttemptError::Failed("multichase 没有输出延迟".to_owned()))
}

/// multiload's best bandwidth, in MiB/s: the `LdMaxMibs` column of the row under its header.
pub fn multiload(output: &Output) -> Result<Value, AttemptError> {
    refused(output)?;
    finished("multiload", output)?;
    let mut lines = output.text.lines();
    let found = lines
        .by_ref()
        .map(fields)
        .find(|header| header.contains(&"LdMaxMibs"))
        .and_then(|header| {
            let column = header.iter().position(|field| *field == "LdMaxMibs")?;
            let row = lines.map(fields).rfind(|row| row.len() == header.len())?;
            value(row[column])
        });
    found.ok_or_else(|| AttemptError::Failed("multiload 没有输出带宽".to_owned()))
}

/// The comma-separated fields of a line of multiload's output, without padding.
fn fields(line: &str) -> Vec<&str> {
    line.split(',').map(str::trim).collect()
}

/// Whether the kernel refused `mbind`, which multichase and multiload report through `perror`.
fn refused(output: &Output) -> Result<(), AttemptError> {
    if output
        .text
        .lines()
        .any(|line| line.trim() == "mbind: Operation not permitted")
    {
        Err(AttemptError::Refused)
    } else {
        Ok(())
    }
}

/// Fails on an exit code other than 0, quoting the tool's last line.
fn finished(tool: &str, output: &Output) -> Result<(), AttemptError> {
    let last = last_line(&output.text);
    match output.code {
        Some(0) => Ok(()),
        Some(code) => Err(AttemptError::Failed(format!(
            "{tool} 退出码 {code}：{last}"
        ))),
        None => Err(AttemptError::Failed(format!("{tool} 被信号终止：{last}"))),
    }
}

/// A finite, positive number.
fn value(text: &str) -> Option<Value> {
    let text = text.trim();
    let value: f64 = text.parse().ok()?;
    (value.is_finite() && value > 0.0).then(|| Value {
        text: text.to_owned(),
        value,
    })
}

/// The last line with text, for a reason that quotes the tool.
fn last_line(output: &str) -> &str {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("没有输出")
}
