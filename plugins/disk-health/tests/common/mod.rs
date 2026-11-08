//! Helpers shared by the tests.

use std::fs;
use std::path::Path;

/// A fixture by its path under `tests/fixtures` (see the README there).
pub fn fixture(path: &str) -> String {
    let full = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path);
    fs::read_to_string(&full).expect("fixtures are committed next to the tests")
}
