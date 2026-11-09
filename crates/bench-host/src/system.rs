//! Kernel release, uptime, load and TCP settings, and the uptime and load the kernel reports
//! itself.

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

/// The uptime and load the running kernel reports itself, which tell whether a container's
/// `/proc/uptime` and `/proc/loadavg` show its own values or the host's: lxcfs can serve those
/// files, but cannot answer system calls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Counters {
    /// `CLOCK_BOOTTIME`. LXD and Incus can intercept sysinfo(2) to report the container's
    /// uptime, but not the clock.
    pub uptime: Duration,
    /// The load averages of sysinfo(2), over 1, 5 and 15 minutes.
    pub load: [f64; 3],
}

impl Counters {
    /// Reads the running kernel's counters: `None` off Linux, and before Linux 2.6.39, which
    /// added `CLOCK_BOOTTIME`.
    #[must_use]
    pub fn read() -> Option<Self> {
        read_counters()
    }

    /// Whether an uptime from `/proc/uptime` is this one, to within a second.
    #[must_use]
    pub fn matches_uptime(&self, shown: Duration) -> bool {
        shown.abs_diff(self.uptime) < Duration::from_secs(1)
    }

    /// Whether load averages from `/proc/loadavg` are these: within 0.01, because the file
    /// rounds them to hundredths.
    #[must_use]
    pub fn matches_load(&self, shown: [f64; 3]) -> bool {
        shown
            .iter()
            .zip(self.load)
            .all(|(shown, kernel)| (shown - kernel).abs() < 0.01)
    }
}

#[cfg(target_os = "linux")]
fn read_counters() -> Option<Counters> {
    use rustix::time::DynamicClockId;

    let boottime = rustix::time::clock_gettime_dynamic(DynamicClockId::Boottime).ok()?;
    let uptime = Duration::new(
        u64::try_from(boottime.tv_sec).ok()?,
        u32::try_from(boottime.tv_nsec).ok()?,
    );
    // sysinfo(2) gives each load as a fixed-point number with 16 fractional bits.
    let load = rustix::system::sysinfo()
        .loads
        .map(|load| load as f64 / 65_536.0);
    Some(Counters { uptime, load })
}

#[cfg(not(target_os = "linux"))]
fn read_counters() -> Option<Counters> {
    None
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
