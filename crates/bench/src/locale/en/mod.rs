//! The English catalog, the one for every locale without a catalog of its own.

mod download;
mod log;
mod program;

/// English.
pub struct English;

/// `count` and the noun, singular for one.
fn count(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}
