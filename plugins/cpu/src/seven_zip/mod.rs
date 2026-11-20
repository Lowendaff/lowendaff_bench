//! 7-Zip LZMA2 integer compression and decompression benchmark (`7zz b`).

pub mod measure;
pub mod parse;
pub mod result;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;
pub use result::Run;
