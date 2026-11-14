//! Compile-time plugin registry, in the default run order. Plugins of the first contract are
//! registered as they are; a plugin of the typed contract is registered with its variant of the
//! result enum, and the reporter names it.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::time::Duration;

use bench_api::RELEASE_ARCHES;
use bench_api::Registration;
use bench_api::Requirements;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use bench_api::typed;
use bench_host::platform::PlatformKind;

use crate::orchestrator::Planned;
use crate::report;
use crate::results::Measured;
use crate::results::Report;

/// Builds a plugin of the typed contract from its config table, ready to run.
type Build = Box<dyn Fn(Option<&serde_json::Value>) -> Result<Planned<Measured>, String>>;

/// Every plugin, in the default run order.
#[must_use]
pub fn all() -> Vec<Entry> {
    vec![
        // 硬盘健康: SMART through smartctl, on dedicated servers only. Before 硬盘性能, so that
        // the disks are idle and fio's writes are not counted.
        legacy::<plugin_disk::health::Smartctl>(),
        // CPU 性能: sysbench, before fio's writes can disturb it.
        entry::<plugin_cpu::sysbench::Sysbench>(Measured::CpuSysbench),
        // 内存性能: sysbench, STREAM and multichase, after CPU 性能 and before fio's writes.
        legacy::<plugin_memory::Memory>(),
        // 硬盘性能: fio with YABS's method, on every platform.
        legacy::<plugin_disk::Fio>(),
        // Geekbench 7: last, being the longest, with a download, an upload and perhaps swap that
        // must not disturb the disk test.
        entry::<plugin_cpu::geekbench::Geekbench>(Measured::CpuGeekbench),
    ]
}

/// One registered plugin.
pub struct Entry {
    /// The plugin's id.
    pub id: &'static str,
    /// The plugin's name: its title in the first contract, or the reporter's name for a plugin
    /// of the typed contract; `None` when the reporter has none.
    pub title: Option<&'static str>,
    /// What the plugin needs.
    pub requires: Requirements,
    /// The limit of one run.
    pub timeout: Duration,
    kind: Kind,
}

enum Kind {
    /// A plugin of the first contract.
    Legacy(Registration),
    /// A plugin of the typed contract.
    Typed(Build),
}

impl Entry {
    /// Builds the plugin from its config table, ready to run; `None` uses its defaults.
    pub fn build(&self, config: Option<&serde_json::Value>) -> Result<Planned<Measured>, String> {
        match &self.kind {
            Kind::Legacy(registration) => Ok(Planned::legacy(
                registration.descriptor,
                registration.build(config)?,
                Measured::Legacy,
            )),
            Kind::Typed(build) => build(config),
        }
    }
}

/// The entry of a plugin of the first contract.
#[must_use]
pub fn legacy<P: bench_api::Plugin>() -> Entry {
    let registration = bench_api::register::<P>();
    let descriptor = registration.descriptor;
    Entry {
        id: descriptor.id,
        title: Some(descriptor.title),
        requires: descriptor.requires,
        timeout: descriptor.timeout,
        kind: Kind::Legacy(registration),
    }
}

/// The entry of a plugin of the typed contract, whose report `wrap` turns into its variant of
/// the result enum.
#[must_use]
pub fn entry<P: typed::Plugin>(wrap: fn(Report<P::Output, P::Failure>) -> Measured) -> Entry {
    let descriptor = P::DESCRIPTOR;
    let title = report::terminal::name(descriptor.id);
    let build = move |config: Option<&serde_json::Value>| {
        let plugin: P = match config {
            None => P::default(),
            Some(value) => serde_json::from_value(value.clone()).map_err(|err| err.to_string())?,
        };
        let mut planned = Planned::typed(plugin, wrap);
        planned.title = title;
        Ok(planned)
    };
    Entry {
        id: descriptor.id,
        title,
        requires: descriptor.requires,
        timeout: descriptor.timeout,
        kind: Kind::Typed(Box::new(build)),
    }
}

/// The contract checks every registry must pass; the binary crate runs them as one test.
/// Messages are for developers, so they are English.
#[must_use]
pub fn validate(entries: &[Entry]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut ids = BTreeSet::new();
    let mut tools: BTreeMap<&str, ToolSpec> = BTreeMap::new();
    let mut by_title: BTreeMap<&str, Vec<(&str, &[PlatformKind])>> = BTreeMap::new();
    for entry in entries {
        let id = entry.id;
        if !valid_id(id) {
            problems.push(format!(
                "{id}: id must be lowercase dotted segments of [a-z0-9-]"
            ));
        }
        if !ids.insert(id) {
            problems.push(format!("{id}: duplicate id"));
        }
        match &entry.kind {
            Kind::Legacy(registration) => {
                let descriptor = registration.descriptor;
                if descriptor.title.is_empty() || descriptor.version.is_empty() {
                    problems.push(format!("{id}: title and version must be set"));
                }
            }
            Kind::Typed(_) => {
                if entry.title.is_none_or(str::is_empty) {
                    problems.push(format!("{id}: the reporter has no name for it"));
                }
            }
        }
        if entry.timeout.is_zero() {
            problems.push(format!("{id}: timeout must be positive"));
        }
        // Variants of one test item share its title; exactly one of them may run anywhere.
        if let Some(title) = entry.title {
            let platforms = entry.requires.platforms;
            let variants = by_title.entry(title).or_default();
            for (other, other_platforms) in variants.iter() {
                if *other == id {
                    // A duplicate registration; the duplicate-id check reports it.
                    continue;
                }
                let overlap = platforms.is_empty()
                    || other_platforms.is_empty()
                    || platforms.iter().any(|kind| other_platforms.contains(kind));
                if overlap {
                    problems.push(format!(
                        "{id}: shares its title with {other}, so their platforms must not overlap"
                    ));
                }
            }
            variants.push((id, platforms));
        }
        if let Err(err) = entry.build(Some(&serde_json::json!({}))) {
            problems.push(format!(
                "{id}: empty config table must build (missing #[serde(default)]?): {err}"
            ));
        }
        if entry
            .build(Some(&serde_json::json!({ "__unknown_field__": true })))
            .is_ok()
        {
            problems.push(format!(
                "{id}: config accepts unknown fields (missing #[serde(deny_unknown_fields)]?)"
            ));
        }
        for tool in entry.requires.tools {
            check_tool(id, tool, &mut problems);
            match tools.get(tool.name) {
                Some(seen) if seen != tool => {
                    problems.push(format!(
                        "{id}: tool {} differs from another plugin's declaration",
                        tool.name
                    ));
                }
                Some(_) => {}
                None => {
                    tools.insert(tool.name, *tool);
                }
            }
        }
    }
    problems
}

fn check_tool(owner: &str, tool: &ToolSpec, problems: &mut Vec<String>) {
    let name = tool.name;
    if name.is_empty() || tool.version.is_empty() {
        problems.push(format!("{owner}: tool name and version must be set"));
    }
    for arch in RELEASE_ARCHES {
        if tool.asset_for(arch).is_none() {
            problems.push(format!("{owner}: tool {name} has no {arch} build"));
        }
    }
    for asset in tool.assets {
        if !asset.url.starts_with("https://") {
            problems.push(format!(
                "{owner}: tool {name} must be downloaded over https"
            ));
        }
        let is_hex = asset
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if asset.sha256.len() != 64 || !is_hex {
            problems.push(format!(
                "{owner}: tool {name} sha256 must be 64 lowercase hex digits"
            ));
        }
        if asset.size == 0 {
            problems.push(format!("{owner}: tool {name} size must be positive"));
        }
    }
    if let ToolForm::TarGz { executable } = tool.form {
        let plain = !executable.is_empty()
            && executable != "."
            && executable != ".."
            && !executable.contains('/');
        if !plain {
            problems.push(format!(
                "{owner}: tool {name} must name its executable in the archive by file name"
            ));
        }
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.split('.').all(|segment| {
            !segment.is_empty()
                && !segment.starts_with('-')
                && !segment.ends_with('-')
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

#[cfg(test)]
mod tests {

    #[test]
    fn registry_passes_the_plugin_contract() {
        assert_eq!(super::validate(&super::all()), Vec::<String>::new());
    }

    #[test]
    fn the_items_run_in_the_owners_order() {
        // SMART is read while the disks are idle and before fio's writes; sysbench runs before
        // fio too.
        let ids: Vec<&str> = super::all().iter().map(|entry| entry.id).collect();
        assert_eq!(
            ids,
            [
                "disk.health",
                "cpu.sysbench",
                "memory",
                "disk.fio",
                "cpu.geekbench",
            ]
        );
    }
}
