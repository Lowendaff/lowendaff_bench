//! y-cruncher multi-threaded Pi computation benchmark (`x86_64` static build).

pub mod measure;
pub mod parse;
pub mod result;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;
