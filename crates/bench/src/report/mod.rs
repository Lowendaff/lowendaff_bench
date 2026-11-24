//! Reporters: each reads the runs and writes them in the requested format (`text`, `markdown`, or
//! `json`), with the catalog's words where human text is shown.

pub mod bgp_graph;
pub mod json;
pub mod markdown;
pub mod terminal;

use terminal::Page;

use crate::vm_cpu::VmCpuSample;

/// Report output format selected by `-f` / `--format`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// Terminal-aligned plain text (the default).
    #[default]
    Text,
    /// GitHub-Flavored Markdown (`--format markdown` or `--format md`).
    #[value(alias = "md")]
    Markdown,
    /// Pretty-printed structured JSON (`--format json`).
    Json,
}

/// Renders `page` in `format`. Only [`Format::Text`] wraps tables to `width`.
#[must_use]
pub fn render(page: &Page, format: Format, width: Option<u16>) -> String {
    render_with_vm_cpu(page, &[], format, width)
}

/// Renders `page` and optional 5-second virtual-machine CPU `%si`/`%st` samples in `format`.
#[must_use]
pub fn render_with_vm_cpu(
    page: &Page,
    vm_cpu_samples: &[VmCpuSample],
    format: Format,
    width: Option<u16>,
) -> String {
    match format {
        Format::Text => terminal::render_with_vm_cpu(page, vm_cpu_samples, width),
        Format::Markdown => markdown::render_with_vm_cpu(page, vm_cpu_samples),
        Format::Json => json::render_with_vm_cpu(page, vm_cpu_samples),
    }
}
