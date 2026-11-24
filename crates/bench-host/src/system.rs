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
    let mut fields = line.split_whitespace().map(|word| {
        word.parse::<f64>()
            .ok()
            .filter(|val| val.is_finite() && *val >= 0.0)
    });
    Some([fields.next()??, fields.next()??, fields.next()??])
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

/// Aggregate kernel CPU time counters from the `cpu ` line of `/proc/stat`, in `USER_HZ` ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTimes {
    /// User mode (already includes `guest` in Linux `account_guest_time`).
    pub user: u64,
    /// Low-priority user mode (already includes `guest_nice`).
    pub nice: u64,
    /// Kernel mode.
    pub system: u64,
    /// Idle task.
    pub idle: u64,
    /// Waiting for I/O completion.
    pub iowait: u64,
    /// Servicing hardware interrupts (`%hi`).
    pub irq: u64,
    /// Servicing software interrupts (`%si`).
    pub softirq: u64,
    /// Involuntary wait while the hypervisor services another virtual CPU (`%st`).
    pub steal: u64,
}

impl CpuTimes {
    /// Reads aggregate CPU counters from `/proc/stat` under `root`.
    #[must_use]
    pub fn read(root: &Root) -> Option<Self> {
        let stat = root.read("/proc/stat")?;
        Self::parse(&stat)
    }

    /// Parses the aggregate `cpu ` line from `/proc/stat` text.
    #[must_use]
    pub fn parse(stat: &str) -> Option<Self> {
        let rest = stat.lines().find_map(|line| line.strip_prefix("cpu "))?;
        let mut fields = rest.split_whitespace();
        let user = fields.next()?.parse::<u64>().ok()?;
        let nice = fields.next()?.parse::<u64>().ok()?;
        let system = fields.next()?.parse::<u64>().ok()?;
        let idle = fields.next()?.parse::<u64>().ok()?;
        let mut next_opt = || match fields.next() {
            Some(word) => word.parse::<u64>().ok(),
            None => Some(0),
        };
        let iowait = next_opt()?;
        let irq = next_opt()?;
        let softirq = next_opt()?;
        let steal = next_opt()?;
        Some(Self {
            user,
            nice,
            system,
            idle,
            iowait,
            irq,
            softirq,
            steal,
        })
    }

    /// Computes `(%si, %st)` over the interval from `earlier` to `self`; `None` when zero ticks
    /// elapsed across all CPU states.
    #[must_use]
    pub fn delta_si_st(&self, earlier: &Self) -> Option<(f64, f64)> {
        let d_user = u128::from(self.user.saturating_sub(earlier.user));
        let d_nice = u128::from(self.nice.saturating_sub(earlier.nice));
        let d_system = u128::from(self.system.saturating_sub(earlier.system));
        let d_idle = u128::from(self.idle.saturating_sub(earlier.idle));
        let d_iowait = u128::from(self.iowait.saturating_sub(earlier.iowait));
        let d_irq = u128::from(self.irq.saturating_sub(earlier.irq));
        let d_softirq = u128::from(self.softirq.saturating_sub(earlier.softirq));
        let d_steal = u128::from(self.steal.saturating_sub(earlier.steal));
        let d_total = d_user + d_nice + d_system + d_idle + d_iowait + d_irq + d_softirq + d_steal;
        if d_total == 0 {
            return None;
        }
        let total = d_total as f64;
        let si = ((d_softirq as f64) * 100.0 / total).clamp(0.0, 100.0);
        let st = ((d_steal as f64) * 100.0 / total).clamp(0.0, 100.0);
        Some((si, st))
    }
}

#[cfg(test)]
mod tests {
    use super::CpuTimes;

    #[test]
    fn cpu_times_excludes_guest_columns_and_handles_nohz_jitter() {
        let t0 =
            CpuTimes::parse("cpu  1000 20 300 8000 50 10 20 100 500 10\ncpu0 1000 20 300 8000\n")
                .expect("valid /proc/stat");
        let t1 = CpuTimes::parse("cpu  1200 20 400 8595 49 10 45 180 600 10\n")
            .expect("valid /proc/stat");
        // Deltas: user=200, nice=0, sys=100, idle=595, iowait=0 (saturated), irq=0, si=25, st=80
        // Total = 1000 ticks -> si = 2.5%, st = 8.0%.
        let (si, st) = t1.delta_si_st(&t0).expect("non-zero delta");
        assert!((si - 2.5).abs() < 1e-9, "expected 2.5% si, got {si}");
        assert!((st - 8.0).abs() < 1e-9, "expected 8.0% st, got {st}");
        assert_eq!(t0.delta_si_st(&t0), None);
    }
}
