//! What a plugin of the first contract left: its records as it emitted them, facts and
//! measurements as labelled lines and tables under their titles.

use bench_api::Record;

use super::Out;
use crate::results::Report;

/// Writes the records.
pub(super) fn block(out: &mut Out, report: &Report<Vec<Record>, String>, indent: &str) {
    for record in report.output.iter().flatten() {
        match record {
            Record::Fact { label, value, .. } => out.field(indent, label, value),
            Record::Metric { label, result, .. } => out.field(indent, label, &result.to_string()),
            Record::Table(table) => {
                out.line(&format!("{indent}{}", table.title()));
                out.table(indent, table.headers(), table.rows());
            }
        }
    }
}
