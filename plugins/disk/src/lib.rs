//! The disk test items: disk health (`disk.health`), the SMART health of a dedicated server's
//! disks, and disk performance (`disk.fio`), fio running YABS's disk test on the filesystem of the
//! test directory, on every platform.

#![warn(missing_docs)]

pub mod fio;
pub mod health;
