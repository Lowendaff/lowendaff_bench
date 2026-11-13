//! The device nodes that smartctl creates to reach MegaRAID disks.

use std::fs;
use std::io;
use std::path::PathBuf;

use bench_host::Root;

/// The nodes smartctl may create: for the megaraid_sas driver, and the old megaraid_mm.
const PATHS: [&str; 2] = ["/dev/megaraid_sas_ioctl_node", "/dev/megadev0"];

/// The nodes that did not exist before the item. Dropping this removes those that exist by
/// then, so they go on every path out of the item, cancellation included.
#[derive(Debug)]
pub struct Nodes {
    missing: Vec<PathBuf>,
}

impl Nodes {
    /// Notes which nodes are missing. Call it before smartctl's first run, because the scan
    /// itself may create a node. Only "not found" counts as missing, so that a node that
    /// existed is never removed.
    #[must_use]
    pub fn note(root: &Root) -> Self {
        let mut missing = Vec::new();
        for path in PATHS {
            let path = root.join(path);
            if fs::symlink_metadata(&path)
                .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
            {
                missing.push(path);
            }
        }
        Self { missing }
    }

    /// Removes the noted nodes that exist now, once; returns those that could not be removed.
    pub fn remove_created(&mut self) -> Vec<(PathBuf, io::Error)> {
        let mut failures = Vec::new();
        for path in self.missing.drain(..) {
            if fs::symlink_metadata(&path).is_err() {
                continue;
            }
            if let Err(error) = fs::remove_file(&path) {
                failures.push((path, error));
            }
        }
        failures
    }
}

impl Drop for Nodes {
    fn drop(&mut self) {
        // After a cancellation nothing reports a failure any more; the normal path calls
        // `remove_created` itself and reports.
        let _ = self.remove_created();
    }
}
