//! The CPU test items: CPU 性能 (`cpu.sysbench`), sysbench's CPU test with one thread and with
//! every usable thread, as the owner's `linux_bench` runs it; and Geekbench 7 (`cpu.geekbench`).

pub mod geekbench;
pub mod sysbench;
