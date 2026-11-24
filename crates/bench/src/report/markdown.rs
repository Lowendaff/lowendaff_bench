//! GitHub-Flavored Markdown report renderer, sharing the section, field and table primitives of
//! [`super::terminal`] so every section and localized phrase stays identical.

use super::terminal;
use super::terminal::Page;
use crate::vm_cpu::VmCpuSample;

/// Renders `page` as GitHub-Flavored Markdown.
#[must_use]
pub fn render(page: &Page) -> String {
    render_with_vm_cpu(page, &[])
}

/// Renders `page` and optional virtual-machine `%si`/`%st` samples as GitHub-Flavored Markdown.
#[must_use]
pub fn render_with_vm_cpu(page: &Page, vm_cpu_samples: &[VmCpuSample]) -> String {
    terminal::render_markdown_with_vm_cpu(page, vm_cpu_samples)
}
