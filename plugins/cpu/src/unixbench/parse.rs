//! Parser and index math for Byte-UnixBench `COUNT|<val>|<timebase>|<unit>` and `TIME|<secs>`.

use bench_tool::job;

use super::result::Fault;
use super::result::IndexItem;

/// Raw count and optional self-reported duration of one UnixBench child process.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CopyReading {
    /// First field of the last `COUNT|<count>|<timebase>|<unit>` line.
    pub count: f64,
    /// Second field (`0` when `count` is already a rate; `1` for seconds; `60` for minutes).
    pub timebase: f64,
    /// Optional `TIME|<secs>` line printed by the child.
    pub reported_secs: Option<f64>,
    /// Measured wall-clock duration of the child in seconds.
    pub elapsed_secs: f64,
}

/// Parses the output of one UnixBench child process for sub-test `item`.
///
/// `fstime -c` prints three `COUNT|` lines (Write, Read, Copy); as in upstream `UnixBench/Run`'s
/// `readResults()`, later `COUNT|` and `TIME|` lines overwrite earlier ones so the final Copy
/// reading wins.
pub fn copy_reading(
    item: &'static str,
    output: &job::Output,
    elapsed_secs: f64,
) -> Result<CopyReading, Fault> {
    if let Some(fault) = output.fault() {
        return Err(Fault::Tool { item, fault });
    }
    let mut last_count: Option<(f64, f64)> = None;
    let mut last_time: Option<f64> = None;
    for line in output.text.lines() {
        let trimmed = line.trim();
        if let Some(idx) = trimmed.find("COUNT|") {
            let rest = &trimmed[idx + "COUNT|".len()..];
            let mut parts = rest.split('|');
            if let (Some(c_str), Some(tb_str)) = (parts.next(), parts.next())
                && let (Ok(count), Ok(tb)) =
                    (c_str.trim().parse::<f64>(), tb_str.trim().parse::<f64>())
                && count.is_finite()
                && count > 0.0
                && tb.is_finite()
                && tb >= 0.0
            {
                last_count = Some((count, tb));
            }
        } else if let Some(rest) = trimmed.strip_prefix("TIME|")
            && let Ok(secs) = rest.trim().parse::<f64>()
            && secs.is_finite()
            && secs > 0.0
        {
            last_time = Some(secs);
        }
    }
    let (count, timebase) = last_count.ok_or(Fault::NoCount { item })?;
    Ok(CopyReading {
        count,
        timebase,
        reported_secs: last_time,
        elapsed_secs: elapsed_secs.max(0.001),
    })
}

/// Combines `readings` from `N` parallel copies into one [`IndexItem`], matching upstream
/// `UnixBench/Run` (`runOnePass` + `combinePassResults` + `indexResults`).
#[must_use]
pub fn combine_copies(
    id: &'static str,
    unit: &'static str,
    baseline: f64,
    readings: &[CopyReading],
) -> Option<IndexItem> {
    if readings.is_empty() || baseline <= 0.0 {
        return None;
    }
    let sum_count: f64 = readings.iter().map(|r| r.count).sum();
    let avg_time: f64 = readings
        .iter()
        .map(|r| r.reported_secs.unwrap_or(r.elapsed_secs).max(0.001))
        .sum::<f64>()
        / (readings.len() as f64);
    let timebase = readings[0].timebase;
    let rate = if timebase > 0.0 {
        sum_count / (avg_time / timebase)
    } else {
        sum_count
    };
    if !rate.is_finite() || rate <= 0.0 {
        return None;
    }
    let index = (rate / baseline) * 10.0;
    Some(IndexItem {
        id,
        rate,
        unit,
        baseline,
        index,
    })
}

/// Computes the overall UnixBench geometric-mean System Index across all 12 items.
#[must_use]
pub fn geometric_index(items: &[IndexItem]) -> Option<f64> {
    if items.is_empty() {
        return None;
    }
    let mut log_sum = 0.0_f64;
    for item in items {
        let ratio = item.rate / item.baseline;
        if !ratio.is_finite() || ratio <= 0.0 {
            return None;
        }
        log_sum += ratio.ln();
    }
    Some((log_sum / (items.len() as f64)).exp() * 10.0)
}
