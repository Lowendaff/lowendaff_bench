//! The system section's words: the host profile's labels, headers and values.

use std::net::IpAddr;
use std::time::Duration;

use bench_host::bmc::Bmc;
use bench_host::cpu::Model;
use bench_host::cpu::Virtualization;
use bench_host::network::Reach;
use bench_host::network::Why;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::raid::Card;
use bench_host::smbios::Ecc;
use bench_host::storage::Use;

/// A label of the system section's fields and tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostLabel {
    /// The hardware, when none of it could be read.
    Hardware,
    /// The machine's vendor and model.
    Machine,
    /// The mainboard.
    Board,
    /// A PC's firmware.
    Bios,
    /// Other firmware, such as U-Boot.
    Firmware,
    /// The baseboard management controller.
    Bmc,
    /// The CPU, when it could not be read.
    Cpu,
    /// The CPU's model.
    CpuModel,
    /// The CPU's threads, cores and sockets.
    CpuCores,
    /// The CPU's clock.
    CpuClock,
    /// The CPU's caches.
    CpuCaches,
    /// The AES instructions.
    Aes,
    /// Hardware virtualization.
    Virtualization,
    /// The memory, when it could not be read.
    Memory,
    /// The memory in all.
    MemoryTotal,
    /// The memory in use.
    MemoryUsed,
    /// The memory in use, as estimated.
    MemoryUsedEstimated,
    /// Swap, when there is none.
    Swap,
    /// The swap in all.
    SwapTotal,
    /// The swap in use.
    SwapUsed,
    /// Error correction.
    Ecc,
    /// The memory modules.
    Modules,
    /// A RAID card.
    Raid,
    /// The disks.
    Disks,
    /// The filesystems.
    Filesystems,
    /// The distribution.
    Os,
    /// The kernel.
    Kernel,
    /// The architecture.
    Arch,
    /// The time since boot.
    Uptime,
    /// The load averages.
    Load,
    /// TCP's congestion control.
    Tcp,
}

/// A header of the system section's tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostHeader {
    /// A disk's kernel name.
    Device,
    /// A disk's interface.
    Interface,
    /// A size.
    Size,
    /// Solid state or rotating.
    Medium,
    /// A model.
    Model,
    /// What a disk is used for.
    Use,
    /// A filesystem's mount point.
    MountPoint,
    /// A filesystem's or a module's type.
    Type,
    /// A filesystem's used space.
    Used,
    /// A filesystem's available space.
    Available,
    /// The disks that a filesystem lies on.
    OnDisks,
    /// A memory module's slot.
    Slot,
    /// A memory module's speed.
    Speed,
    /// A memory module's vendor.
    Vendor,
}

/// The logical CPUs, as each kind of platform tells them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cores {
    /// A dedicated server's threads, with its sockets and physical cores when known.
    Dedicated {
        /// Logical CPUs.
        threads: usize,
        /// Sockets and physical cores in all.
        topology: Option<(usize, usize)>,
    },
    /// A virtual machine's virtual CPUs.
    Vm(usize),
    /// A container's usable CPUs, with its quota when it has one.
    Container {
        /// CPUs that it may run on.
        usable: usize,
        /// Its CPU quota, in CPUs.
        quota: Option<f64>,
    },
}

/// Why there is no table of memory modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoModules<'a> {
    /// The device tree's memory, as a size, and no modules.
    Board(&'a str),
    /// SMBIOS describes no modules.
    NotInSmbios,
    /// There is no SMBIOS.
    NoSmbios,
    /// SMBIOS could not be read through `/dev/mem`.
    DevMem(&'a str),
}

/// Why ECC is unknown, when neither SMBIOS nor EDAC tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoEcc {
    /// SMBIOS has no word on error correction.
    NotInSmbios,
    /// There is no SMBIOS.
    NoSmbios,
    /// SMBIOS could not be read.
    SmbiosUnreadable,
}

/// The system section's words.
pub trait HostWords {
    /// A label.
    fn host_label(&self, label: HostLabel) -> &'static str;
    /// A table's header.
    fn host_header(&self, header: HostHeader) -> &'static str;
    /// Where the program runs.
    fn platform(&self, platform: Platform) -> String;
    /// A kind of platform, as `--list` names the ones that a test item applies to.
    fn platform_kind(&self, kind: PlatformKind) -> &'static str;
    /// A part of the profile whose reading ran out of time.
    fn read_timed_out(&self) -> String;
    /// A value that there is none of, as swap, RAID cards or ECC can be.
    fn none(&self) -> &'static str;
    /// Whether the CPU has the AES instructions.
    fn aes(&self, supported: bool) -> &'static str;
    /// Hardware virtualization.
    fn virtualization(&self, virtualization: Virtualization) -> &'static str;
    /// A CPU's model, or its codes when the table does not know it.
    fn cpu_model(&self, model: &Model) -> String;
    /// The logical CPUs.
    fn cores(&self, cores: Cores) -> String;
    /// A name with its detail, such as a board's model with its version.
    fn with_detail(&self, name: &str, detail: &str) -> String;
    /// U-Boot's version.
    fn u_boot(&self, version: &str) -> String;
    /// The Raspberry Pi's bootloader, with the date it was built.
    fn raspberry_pi(&self, built: &str) -> String;
    /// The BMC, or that none was found.
    fn bmc(&self, bmc: Option<&Bmc>) -> String;
    /// ECC: the ways of correction, and the counts since boot, corrected and uncorrected.
    fn ecc(&self, modes: &[Ecc], counts: Option<(u64, u64)>) -> String;
    /// ECC, when no way of correction is known and there are no counts.
    fn no_ecc(&self, why: NoEcc) -> String;
    /// Why there is no table of memory modules.
    fn no_modules(&self, why: NoModules<'_>) -> String;
    /// A slot without a name, by its number from 1.
    fn slot(&self, number: usize) -> String;
    /// A slot without a module.
    fn empty_slot(&self) -> &'static str;
    /// A slot whose module the firmware reports as not in use.
    fn disabled_module(&self) -> &'static str;
    /// A module's speed: as configured, and as rated when that differs.
    fn module_speed(&self, configured: Option<u32>, rated: Option<u32>) -> Option<String>;
    /// The note on the slots whose module is not in use.
    fn disabled_modules(&self, slots: &[String]) -> String;
    /// A RAID card, and how its disks attach to it.
    fn raid_card(&self, card: &Card) -> String;
    /// A disk's medium: solid state, rotating, or unknown.
    fn medium(&self, rotational: Option<bool>) -> &'static str;
    /// What a disk is used for.
    fn uses(&self, uses: &[Use]) -> String;
    /// The mark of the filesystem that holds the test directory.
    fn test_directory(&self) -> &'static str;
    /// The mark of a value that is the host's, seen from a container.
    fn of_the_host(&self) -> &'static str;
    /// The time since boot.
    fn uptime(&self, uptime: Duration) -> String;
    /// TCP's congestion control, with the queue discipline when known.
    fn tcp(&self, algorithm: &str, qdisc: Option<&str>) -> String;
    /// A family that works, without asking for its public address.
    fn reachable(&self) -> String;
    /// A family that works, with its public address and the source that told it.
    fn public_address(&self, address: &str, source: &str) -> String;
    /// A family that works, without a public address: each source's reason.
    fn no_public_address(&self, reasons: &[String]) -> String;
    /// A host without a route for the family, such as `IPv6`.
    fn no_route(&self, family: &str) -> String;
    /// How a connection to one of the family's targets went.
    fn reach(&self, ip: IpAddr, reach: Reach) -> String;
    /// Why a source gave no address of the family.
    fn why(&self, source: &str, why: Why, family: &str) -> String;
}
