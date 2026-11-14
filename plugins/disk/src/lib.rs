//! The disk test items: 硬盘健康 (`disk.health`), the SMART health of a dedicated server's disks,
//! and 硬盘性能 (`disk.fio`), fio running YABS's disk test on the filesystem of the test
//! directory, on every platform.

pub mod fio;
pub mod health;
