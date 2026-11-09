//! EDAC's memory controllers (system information design §9.4): the errors corrected and not
//! corrected since the driver loaded, and how the controllers correct them. EDAC's list of
//! modules is not used: on the owner's server it names the wrong module type.

use crate::Root;
use crate::smbios::Ecc;

/// What EDAC reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edac {
    /// Corrected errors, summed over the controllers.
    pub corrected: u64,
    /// Uncorrected errors, summed over the controllers.
    pub uncorrected: u64,
    /// How the controllers' memory corrects errors, each mode once, in controller order.
    pub modes: Vec<Ecc>,
}

/// The controllers' counts and modes, or `None` when no EDAC driver is loaded.
#[must_use]
pub fn read(root: &Root) -> Option<Edac> {
    let base = "/sys/devices/system/edac/mc";
    let controllers: Vec<String> = root
        .list(base)
        .into_iter()
        .filter(|name| {
            name.strip_prefix("mc").is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
        .collect();
    if controllers.is_empty() {
        return None;
    }
    let mut edac = Edac {
        corrected: 0,
        uncorrected: 0,
        modes: Vec::new(),
    };
    for controller in &controllers {
        let path = format!("{base}/{controller}");
        let count = |name: &str| {
            root.read_line(&format!("{path}/{name}"))
                .and_then(|text| text.parse::<u64>().ok())
                .unwrap_or(0)
        };
        edac.corrected = edac.corrected.saturating_add(count("ce_count"));
        edac.uncorrected = edac.uncorrected.saturating_add(count("ue_count"));
        // Modules are `dimm*`, or `rank*` on drivers that count ranks; old drivers have `csrow*`.
        for entry in root.list(&path) {
            let file = if entry.starts_with("dimm") || entry.starts_with("rank") {
                "dimm_edac_mode"
            } else if entry.starts_with("csrow") {
                "edac_mode"
            } else {
                continue;
            };
            let Some(mode) = root
                .read_line(&format!("{path}/{entry}/{file}"))
                .and_then(|text| mode(&text))
            else {
                continue;
            };
            if !edac.modes.contains(&mode) {
                edac.modes.push(mode);
            }
        }
    }
    Some(edac)
}

/// EDAC's names for the modes (`edac_caps` in the kernel's `edac_mc_sysfs.c`).
fn mode(text: &str) -> Option<Ecc> {
    match text.to_ascii_uppercase().as_str() {
        "SECDED" => Some(Ecc::SingleBit),
        "S2ECD2ED" | "S4ECD4ED" | "S8ECD8ED" | "S16ECD16ED" => Some(Ecc::MultiBit),
        "PARITY" => Some(Ecc::Parity),
        "NONE" => Some(Ecc::None),
        _ => None,
    }
}
