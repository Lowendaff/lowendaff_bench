//! The distribution's name, from os-release(5) with fallbacks for older systems.

use std::collections::BTreeMap;

use crate::Root;

/// The distribution's display name, such as "Debian GNU/Linux 13 (trixie)".
///
/// os-release(5) is read from `/etc`, then `/usr/lib`. Its `PRETTY_NAME` comes first, then
/// `NAME` with `VERSION` or `VERSION_ID`. Systems without os-release fall back to the
/// release files that predate it.
#[must_use]
pub fn distribution(root: &Root) -> Option<String> {
    let release = root
        .read("/etc/os-release")
        .or_else(|| root.read("/usr/lib/os-release"))
        .map(|text| os_release(&text));
    if let Some(fields) = &release {
        if let Some(pretty) = fields.get("PRETTY_NAME").filter(|name| !name.is_empty()) {
            return Some(pretty.clone());
        }
        if let Some(name) = fields.get("NAME").filter(|name| !name.is_empty()) {
            let version = fields.get("VERSION").or_else(|| fields.get("VERSION_ID"));
            return Some(match version.filter(|version| !version.is_empty()) {
                Some(version) => format!("{name} {version}"),
                None => name.clone(),
            });
        }
    }
    if let Some(description) = root
        .read("/etc/lsb-release")
        .and_then(|text| os_release(&text).remove("DISTRIB_DESCRIPTION"))
        .filter(|description| !description.is_empty())
    {
        return Some(description);
    }
    for path in [
        "/etc/redhat-release",
        "/etc/centos-release",
        "/etc/system-release",
    ] {
        if let Some(line) = root.read_line(path) {
            return Some(line);
        }
    }
    if let Some(version) = root.read_line("/etc/debian_version") {
        return Some(format!("Debian {version}"));
    }
    root.read_line("/etc/alpine-release")
        .map(|version| format!("Alpine Linux {version}"))
}

/// Parses os-release(5) assignments: values may be unquoted, or in single or double
/// quotes; inside double quotes `\` escapes `"`, `\`, `$` and `` ` ``.
fn os_release(text: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        fields.insert(key.trim().to_owned(), unquote(value.trim()));
    }
    fields
}

fn unquote(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    let mut quote = None;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            (Some('"') | None, '\\') => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            (_, c) => out.push(c),
        }
    }
    out
}
