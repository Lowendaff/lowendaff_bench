//! The 硬盘 table and its 说明 lines.

use bench_api::Record;
use bench_api::RecordError;
use bench_api::Table;
use bench_host::Root;

use crate::health::location::Location;
use crate::health::reading::Reading;

/// What a missing value shows.
const MISSING: &str = "—";

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Where the disk sits.
    pub location: Location,
    /// What smartctl reported.
    pub reading: Reading,
}

/// One row per physical disk, in table order. Readings with the same serial are one disk
/// listed twice: a named device is kept over a RAID-card path, otherwise the first in scan
/// order. Readings without a serial are never merged.
#[must_use]
pub fn rows(readings: Vec<Reading>, root: &Root) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for reading in readings {
        let row = Row {
            location: Location::of(&reading.device, root),
            reading,
        };
        let same =
            |kept: &Row| kept.reading.serial.is_some() && kept.reading.serial == row.reading.serial;
        match rows.iter().position(same) {
            Some(index) => {
                let named = matches!(row.location, Location::Named(_));
                if named && matches!(rows[index].location, Location::Card { .. }) {
                    rows[index] = row;
                }
            }
            None => rows.push(row),
        }
    }
    // Stable, so disks with the same place keep the scan's order.
    rows.sort_by(|a, b| a.location.cmp(&b.location));
    rows
}

/// The 硬盘 table.
pub fn table(rows: &[Row]) -> Result<Record, RecordError> {
    let headers = [
        "设备",
        "健康",
        "通电时间",
        "温度",
        "已用寿命",
        "累计写入",
        "坏块",
        "接口错误",
    ];
    let with_host = several_cards(rows);
    let cells = rows.iter().map(|row| cells(row, with_host)).collect();
    let table = Table::try_new("disks", "硬盘", headers.map(str::to_owned).to_vec(), cells)?;
    Ok(Record::Table(table))
}

/// One 说明 line for each row that needs one, in row order.
#[must_use]
pub fn notes(rows: &[Row]) -> Vec<Record> {
    let with_host = several_cards(rows);
    let mut notes = Vec::new();
    for row in rows {
        let Some(note) = row.reading.health.note() else {
            continue;
        };
        let id = format!("note-{}", notes.len() + 1);
        let name = row.location.name(with_host);
        notes.push(Record::fact(&id, "说明", format!("{name} {note}")));
    }
    notes
}

/// Bytes in decimal units with one decimal: GB below 1000 GB, TB below 1000 TB, then PB.
/// Extra digits are cut, not rounded, so 999,999,999,999 bytes show as `999.9 GB`.
#[must_use]
pub fn written(bytes: u64) -> String {
    let (unit, tenth) = if bytes < 1_000_000_000_000 {
        ("GB", 100_000_000)
    } else if bytes < 1_000_000_000_000_000 {
        ("TB", 100_000_000_000)
    } else {
        ("PB", 100_000_000_000_000)
    };
    let tenths = bytes / tenth;
    format!("{}.{} {unit}", tenths / 10, tenths % 10)
}

/// Whether the RAID-card disks in the table sit on more than one card.
fn several_cards(rows: &[Row]) -> bool {
    let mut hosts = rows.iter().filter_map(|row| match row.location {
        Location::Card { host, .. } => Some(host),
        Location::Named(_) => None,
    });
    hosts
        .next()
        .is_some_and(|first| hosts.any(|host| host != first))
}

fn cells(row: &Row, with_host: bool) -> Vec<String> {
    let values = &row.reading.values;
    vec![
        row.location.name(with_host),
        row.reading.health.to_string(),
        shown(values.hours, |hours| format!("{hours} 小时")),
        shown(values.temperature, |degrees| format!("{degrees} °C")),
        shown(values.endurance_used, |percent| format!("{percent}%")),
        shown(values.written, written),
        values.bad_blocks.to_string(),
        shown(values.interface_errors, |count| count.to_string()),
    ]
}

fn shown<T>(value: Option<T>, format: impl FnOnce(T) -> String) -> String {
    value.map_or_else(|| MISSING.to_owned(), format)
}
