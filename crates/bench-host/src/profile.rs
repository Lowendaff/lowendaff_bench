//! The host profile: what the program reads about its host once, before any plugin runs, and
//! then shares read-only. Each section is read on a thread of its own, and the whole waits at
//! most a budget, 30 s on a real host: a section that is stuck, for example in statvfs on a dead
//! network mount, is reported as timed out and left behind, and the run goes on. The network
//! part is the program's to fill in, since this crate connects nowhere.

use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

use crate::Root;
use crate::cpu;
use crate::cpu::Cpu;
use crate::glibc;
use crate::glibc::Libc;
use crate::hardware::Hardware;
use crate::memory;
use crate::network::Network;
use crate::numa::Layout;
use crate::os;
use crate::platform;
use crate::platform::ContainerKind;
use crate::platform::Platform;
use crate::platform::PlatformKind;
use crate::raid;
use crate::raid::Card;
use crate::storage;
use crate::storage::Disk;
use crate::storage::Filesystem;
use crate::storage::Space;
use crate::system;
use crate::system::Counters;

/// How long [`HostProfile::collect`] waits for its sections: as long as the system item of
/// earlier versions was allowed to run.
pub const BUDGET: Duration = Duration::from_secs(30);

/// Everything the run knows about its host.
#[derive(Debug, Default)]
pub struct HostProfile {
    /// Where the program runs.
    pub platform: Platform,
    /// `std::env::consts::ARCH`.
    pub arch: String,
    /// The C library, which tools built against glibc need.
    pub libc: Libc,
    /// The CPU.
    pub cpu: Read<CpuFacts>,
    /// Memory and swap; `None` inside when `/proc/meminfo` cannot be read.
    pub memory: Read<Option<MemoryFacts>>,
    /// The NUMA nodes, and the CPUs and nodes this process may use.
    pub numa: Read<Layout>,
    /// Disks, filesystems and RAID cards.
    pub storage: Read<StorageFacts>,
    /// Distribution, kernel, uptime, load and TCP.
    pub system: Read<SystemFacts>,
    /// A dedicated server's hardware; `None` on other platforms.
    pub hardware: Option<Read<Hardware>>,
    /// Reachability and the public addresses, filled in by the program's network module.
    pub network: Network,
}

/// A section of the profile, or why it is missing.
#[derive(Debug, Clone, PartialEq)]
pub enum Read<T> {
    /// The section as read.
    Done(T),
    /// The section had not been read when the budget ran out; its thread is left behind.
    TimedOut,
    /// The section's thread ended without an answer.
    Failed,
}

impl<T: Default> Default for Read<T> {
    fn default() -> Self {
        Self::Done(T::default())
    }
}

/// The CPU, and what a test may use of it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuFacts {
    /// What the CPU reports about itself.
    pub cpu: Cpu,
    /// The CPUs a test can use at once: the logical CPUs, or a container's cpuset.
    pub usable: Option<usize>,
    /// A container's CPU quota, such as 1.5, when it is below [`Self::usable`].
    pub quota: Option<f64>,
}

/// Memory and swap in bytes. A container's come from its cgroup when its limit is below what
/// `/proc/meminfo` shows, which is then the host's (Docker); with lxcfs (LXC), `/proc/meminfo`
/// already shows the container's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryFacts {
    /// Total memory.
    pub total: u64,
    /// Memory in use.
    pub used: u64,
    /// Whether `used` is estimated, because the kernel reports no `MemAvailable`.
    pub estimated: bool,
    /// Total swap.
    pub swap_total: u64,
    /// Swap in use.
    pub swap_used: u64,
}

/// Disks, filesystems and RAID cards.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageFacts {
    /// The disks; none in a container, which would see its host's.
    pub disks: Vec<Disk>,
    /// The mounted filesystems; in a container only its root, sized by its own quota.
    pub filesystems: Vec<Filesystem>,
    /// The RAID-class controllers and their disks, on dedicated servers.
    pub raid: Option<Vec<Card>>,
    /// `major:minor` of the filesystem that holds the work directory.
    pub work_dir: Option<(u32, u32)>,
}

/// The system and its kernel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemFacts {
    /// The distribution, such as `Debian GNU/Linux 13 (trixie)`.
    pub distribution: Option<String>,
    /// The kernel release.
    pub kernel: Option<String>,
    /// The uptime that `/proc/uptime` shows.
    pub uptime: Option<Seen<Duration>>,
    /// The load averages that `/proc/loadavg` shows, over 1, 5 and 15 minutes.
    pub load: Option<Seen<[f64; 3]>>,
    /// The TCP congestion control algorithm, such as `bbr`.
    pub congestion_control: Option<String>,
    /// The default queueing discipline, such as `fq`.
    pub qdisc: Option<String>,
}

/// A value as `/proc` shows it, and whether it is the host's rather than the container's own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seen<T> {
    /// The value.
    pub value: T,
    /// Whether it equals the running kernel's own, in a container that should see its own.
    pub host: bool,
}

/// Where a profile is read from: the host's files, what is known beforehand, and the two
/// readers that are not files, which tests stand in for.
pub struct Sources<F, K> {
    /// The host's root, or a captured fixture.
    pub root: Root,
    /// `std::env::consts::ARCH`.
    pub arch: String,
    /// Where the program runs.
    pub platform: Platform,
    /// The C library.
    pub libc: Libc,
    /// `major:minor` of the filesystem that holds the work directory.
    pub work_dir: Option<(u32, u32)>,
    /// The size of a filesystem by mount point: [`storage::statvfs`] on a real host.
    pub space: F,
    /// The running kernel's own uptime and load: [`Counters::read`] on a real host.
    pub kernel: K,
}

/// A section of the profile, for its thread.
#[derive(Clone, Copy)]
enum Section {
    Cpu,
    Memory,
    Numa,
    Storage,
    System,
    Hardware,
}

/// One section, as its thread sends it.
enum Part {
    Cpu(CpuFacts),
    Memory(Option<MemoryFacts>),
    Numa(Layout),
    Storage(StorageFacts),
    System(SystemFacts),
    Hardware(Box<Hardware>),
}

impl HostProfile {
    /// Reads this host, with the work directory under `work_dir`, in at most [`BUDGET`].
    #[must_use]
    pub fn collect(work_dir: &Path) -> Self {
        let root = Root::system();
        let arch = std::env::consts::ARCH;
        let sources = Sources {
            platform: platform::detect(&root, platform::cpuid().as_ref()),
            libc: glibc::read(&root, arch),
            arch: arch.to_owned(),
            work_dir: storage::device_of(work_dir),
            space: storage::statvfs,
            kernel: Counters::read,
            root,
        };
        Self::read(sources, BUDGET)
    }

    /// Reads a profile from `sources`, waiting at most `budget` for its sections.
    #[must_use]
    pub fn read<F, K>(sources: Sources<F, K>, budget: Duration) -> Self
    where
        F: Fn(&str) -> Option<Space> + Send + 'static,
        K: Fn() -> Option<Counters> + Send + 'static,
    {
        let Sources {
            root,
            arch,
            platform,
            libc,
            work_dir,
            space,
            kernel,
        } = sources;
        let deadline = Instant::now() + budget;
        let kind = platform.kind();
        let (sender, receiver) = mpsc::channel();
        let mut threads = Vec::new();
        {
            let root = root.clone();
            threads.push((
                Section::Cpu,
                spawn(&sender, "host-cpu", move || {
                    Part::Cpu(cpu_facts(&root, kind))
                }),
            ));
        }
        {
            let root = root.clone();
            threads.push((
                Section::Memory,
                spawn(&sender, "host-memory", move || {
                    Part::Memory(memory_facts(&root, kind))
                }),
            ));
        }
        {
            let root = root.clone();
            threads.push((
                Section::Numa,
                spawn(&sender, "host-numa", move || {
                    Part::Numa(Layout::read(&root))
                }),
            ));
        }
        {
            let root = root.clone();
            threads.push((
                Section::Storage,
                spawn(&sender, "host-storage", move || {
                    Part::Storage(storage_facts(&root, kind, work_dir, space))
                }),
            ));
        }
        {
            let root = root.clone();
            threads.push((
                Section::System,
                spawn(&sender, "host-system", move || {
                    Part::System(system_facts(&root, platform, kernel))
                }),
            ));
        }
        if kind == PlatformKind::Dedicated {
            let arch = arch.clone();
            threads.push((
                Section::Hardware,
                spawn(&sender, "host-hardware", move || {
                    Part::Hardware(Box::new(Hardware::read(&root, &arch)))
                }),
            ));
        }
        drop(sender);

        let mut profile = Self {
            platform,
            arch,
            libc,
            cpu: Read::Failed,
            memory: Read::Failed,
            numa: Read::Failed,
            storage: Read::Failed,
            system: Read::Failed,
            hardware: (kind == PlatformKind::Dedicated).then_some(Read::Failed),
            network: Network::default(),
        };
        let mut timed_out = false;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match receiver.recv_timeout(left) {
                Ok(part) => profile.fill(part),
                Err(RecvTimeoutError::Timeout) => {
                    timed_out = true;
                    break;
                }
                // Every thread has ended, so a section still missing has failed.
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        // A part may have come in just as the budget ran out.
        while let Ok(part) = receiver.try_recv() {
            profile.fill(part);
        }
        // When the budget ran out, a section still missing timed out if its thread is still
        // running, and failed if its thread ended, or never started, without sending it.
        if timed_out {
            for (section, thread) in &threads {
                if thread.as_ref().is_some_and(|thread| !thread.is_finished()) {
                    profile.time_out(*section);
                }
            }
        }
        profile
    }

    fn fill(&mut self, part: Part) {
        match part {
            Part::Cpu(cpu) => self.cpu = Read::Done(cpu),
            Part::Memory(memory) => self.memory = Read::Done(memory),
            Part::Numa(layout) => self.numa = Read::Done(layout),
            Part::Storage(storage) => self.storage = Read::Done(storage),
            Part::System(system) => self.system = Read::Done(system),
            Part::Hardware(hardware) => self.hardware = Some(Read::Done(*hardware)),
        }
    }

    /// Marks a section as timed out, unless it came in.
    fn time_out(&mut self, section: Section) {
        fn out<T>(read: &mut Read<T>) {
            if matches!(read, Read::Failed) {
                *read = Read::TimedOut;
            }
        }
        match section {
            Section::Cpu => out(&mut self.cpu),
            Section::Memory => out(&mut self.memory),
            Section::Numa => out(&mut self.numa),
            Section::Storage => out(&mut self.storage),
            Section::System => out(&mut self.system),
            Section::Hardware => {
                if let Some(hardware) = &mut self.hardware {
                    out(hardware);
                }
            }
        }
    }
}

/// Reads a section on a thread of its own; `None` when the thread cannot be started, and the
/// section is then missing.
fn spawn(
    sender: &mpsc::Sender<Part>,
    name: &str,
    read: impl FnOnce() -> Part + Send + 'static,
) -> Option<JoinHandle<()>> {
    let sender = sender.clone();
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            // The receiver is gone only once the budget has run out.
            let _ = sender.send(read());
        })
        .ok()
}

fn cpu_facts(root: &Root, kind: PlatformKind) -> CpuFacts {
    let cpu = cpu::read(root);
    let limits = (kind == PlatformKind::Container).then(|| memory::cgroup_limits(root));
    let usable = cpu
        .logical
        .map(|logical| cpu::threads(logical, kind, limits.and_then(|limits| limits.cpuset)));
    let quota = limits
        .and_then(|limits| limits.cpus)
        .filter(|quota| usable.is_some_and(|usable| *quota < usable as f64));
    CpuFacts { cpu, usable, quota }
}

fn memory_facts(root: &Root, kind: PlatformKind) -> Option<MemoryFacts> {
    let memory = memory::read(root)?;
    let limits = (kind == PlatformKind::Container).then(|| memory::cgroup_limits(root));
    let cgroup = limits.filter(|limits| limits.memory.is_some_and(|limit| limit < memory.total));
    let (total, used) = match cgroup {
        Some(limits) => (
            limits.memory.unwrap_or(memory.total),
            limits.memory_used.unwrap_or_else(|| memory.used()),
        ),
        None => (memory.total, memory.used()),
    };
    let (swap_total, swap_used) = match cgroup.and_then(|limits| limits.swap) {
        Some(swap) => (
            swap,
            cgroup.and_then(|limits| limits.swap_used).unwrap_or(0),
        ),
        None => (memory.swap_total, memory.swap_used()),
    };
    Some(MemoryFacts {
        total,
        used,
        estimated: memory.estimated,
        swap_total,
        swap_used,
    })
}

fn storage_facts(
    root: &Root,
    kind: PlatformKind,
    work_dir: Option<(u32, u32)>,
    space: impl Fn(&str) -> Option<Space>,
) -> StorageFacts {
    if kind == PlatformKind::Container {
        // A container sees its host's disks, so only its own root filesystem counts; a ZFS
        // dataset there is sized by the container's quota, so its own numbers stand.
        let filesystems = storage::root_filesystem(root, &space)
            .map(|root| Filesystem {
                zfs_pool: None,
                ..root
            })
            .into_iter()
            .collect();
        return StorageFacts {
            disks: Vec::new(),
            filesystems,
            raid: None,
            work_dir,
        };
    }
    let storage = storage::read(root, &space);
    let raid = (kind == PlatformKind::Dedicated).then(|| {
        let disks: Vec<String> = storage.disks.iter().map(|disk| disk.name.clone()).collect();
        raid::read(root, &disks)
    });
    StorageFacts {
        disks: storage.disks,
        filesystems: storage.filesystems,
        raid,
        work_dir,
    }
}

fn system_facts(
    root: &Root,
    platform: Platform,
    kernel: impl Fn() -> Option<Counters>,
) -> SystemFacts {
    // A container's /proc may show its host's uptime and load: Docker shares the kernel's, and
    // lxcfs passes the load through. OpenVZ's kernel answers with the container's own.
    let compare =
        matches!(platform, Platform::Container(container) if container != ContainerKind::Openvz);
    // The kernel is read before and after /proc, because its load averages change every five
    // seconds: /proc matches one of the readings even when a change falls in between.
    let read = || if compare { kernel() } else { None };
    let before = read();
    let (uptime, load) = (system::uptime(root), system::load(root));
    let after = read();
    let host = |matches: &dyn Fn(&Counters) -> bool| before.iter().chain(&after).any(matches);
    SystemFacts {
        distribution: os::distribution(root),
        kernel: system::kernel(root),
        uptime: uptime.map(|value| Seen {
            value,
            host: host(&|kernel| kernel.matches_uptime(value)),
        }),
        load: load.map(|value| Seen {
            value,
            host: host(&|kernel| kernel.matches_load(value)),
        }),
        congestion_control: system::congestion_control(root),
        qdisc: system::default_qdisc(root),
    }
}
