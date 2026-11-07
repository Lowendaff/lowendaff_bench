//! Kernel release, uptime, load and TCP settings.

use std::time::Duration;

use crate::Root;

/// The kernel release, as `uname -r` prints it.
#[must_use]
pub fn kernel(root: &Root) -> Option<String> {
    root.read_line("/proc/sys/kernel/osrelease")
}

/// Time since boot.
#[must_use]
pub fn uptime(root: &Root) -> Option<Duration> {
    let line = root.read_line("/proc/uptime")?;
    let seconds: f64 = line.split_whitespace().next()?.parse().ok()?;
    Duration::try_from_secs_f64(seconds).ok()
}

/// Load averages over 1, 5 and 15 minutes.
#[must_use]
pub fn load(root: &Root) -> Option<[f64; 3]> {
    let line = root.read_line("/proc/loadavg")?;
    let mut fields = line.split_whitespace().map(str::parse::<f64>);
    Some([
        fields.next()?.ok()?,
        fields.next()?.ok()?,
        fields.next()?.ok()?,
    ])
}

/// TCP congestion control algorithm, such as `bbr`.
#[must_use]
pub fn congestion_control(root: &Root) -> Option<String> {
    root.read_line("/proc/sys/net/ipv4/tcp_congestion_control")
}

/// Default queueing discipline, such as `fq`. Missing inside network namespaces.
#[must_use]
pub fn default_qdisc(root: &Root) -> Option<String> {
    root.read_line("/proc/sys/net/core/default_qdisc")
}
