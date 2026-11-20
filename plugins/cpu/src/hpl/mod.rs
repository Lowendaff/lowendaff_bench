//! High-Performance Linpack (`xhpl` + OpenBLAS) dense double-precision LU GFLOPS benchmark.

pub mod measure;
pub mod parse;
pub mod result;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;
