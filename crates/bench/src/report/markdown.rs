//! GitHub-Flavored Markdown report renderer, sharing the section, field and table primitives of
//! [`super::terminal`] so every section and localized phrase stays identical.

use super::terminal;
use super::terminal::Page;

/// Renders `page` as GitHub-Flavored Markdown.
#[must_use]
pub fn render(page: &Page) -> String {
    terminal::render_markdown(page)
}
