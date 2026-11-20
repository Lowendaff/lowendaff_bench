//! The English catalog, the one for every locale without a catalog of its own.

mod download;
mod host;
mod items;
mod log;
mod program;
mod report;
mod tui;

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
