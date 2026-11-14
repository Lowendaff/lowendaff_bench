//! Helpers shared by the tests.

use std::fs;
use std::path::Path;

/// A fixture of 硬盘健康 by its path under `tests/fixtures/health` (see the README there).
pub fn fixture(path: &str) -> String {
    let full = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/health")
        .join(path);
    fs::read_to_string(&full).expect("fixtures are committed next to the tests")
}
