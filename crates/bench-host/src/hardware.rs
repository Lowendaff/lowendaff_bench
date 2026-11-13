//! A dedicated server's hardware: the SMBIOS table, the machine with its board and firmware,
//! the BMC and EDAC. They are read together because the machine and the BMC look into the
//! SMBIOS table as well.

use crate::Root;
use crate::bmc;
use crate::bmc::Bmc;
use crate::edac;
use crate::edac::Edac;
use crate::machine;
use crate::machine::Machine;
use crate::smbios;
use crate::smbios::Smbios;
use crate::smbios::Unreadable;

/// What the hardware lines of a dedicated server are made of.
#[derive(Debug)]
pub struct Hardware {
    /// The SMBIOS table, or why there is none.
    pub smbios: Result<Smbios, Unreadable>,
    /// The machine, its board and its firmware.
    pub machine: Machine,
    /// The BMC, or `None` when none was found.
    pub bmc: Option<Bmc>,
    /// EDAC's counts and modes, or `None` without an EDAC driver.
    pub edac: Option<Edac>,
    /// The memory the device tree describes, in bytes.
    pub board_memory: Option<u64>,
}

impl Hardware {
    /// Reads the host; `arch` is `std::env::consts::ARCH`.
    #[must_use]
    pub fn read(root: &Root, arch: &str) -> Self {
        let smbios = smbios::read(root, arch);
        let table = smbios.as_ref().ok();
        Self {
            machine: machine::read(root, table),
            bmc: bmc::read(root, table),
            edac: edac::read(root),
            board_memory: machine::board_memory(root),
            smbios,
        }
    }
}
