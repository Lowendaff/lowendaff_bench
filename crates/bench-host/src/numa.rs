//! NUMA nodes: the CPUs and the memory of each, and the nodes this process may allocate on
//! (design memory, §6).

use crate::Root;
use crate::memory;

/// One NUMA node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Its number, as in `node1`.
    pub id: usize,
    /// Its CPUs, ascending; none for a node of memory only.
    pub cpus: Vec<usize>,
    /// Its memory in bytes, or `None` when unknown.
    pub total: Option<u64>,
    /// Its free memory in bytes, or `None` when unknown.
    pub free: Option<u64>,
}

/// The nodes under `root`, by number; empty when the kernel has no NUMA support.
#[must_use]
pub fn read(root: &Root) -> Vec<Node> {
    const NODES: &str = "/sys/devices/system/node";
    let mut nodes: Vec<Node> = root
        .list(NODES)
        .into_iter()
        .filter_map(|name| {
            let id = name.strip_prefix("node")?.parse().ok()?;
            let dir = format!("{NODES}/{name}");
            let text = root.read(&format!("{dir}/meminfo")).unwrap_or_default();
            let fields = memory::meminfo(&text);
            let field = |key: &str| fields.get(format!("Node {id} {key}").as_str()).copied();
            Some(Node {
                id,
                cpus: root
                    .read_line(&format!("{dir}/cpulist"))
                    .and_then(|list| crate::cpu_list(&list))
                    .unwrap_or_default(),
                total: field("MemTotal"),
                free: field("MemFree"),
            })
        })
        .collect();
    nodes.sort_by_key(|node| node.id);
    nodes
}

/// The nodes this process may allocate memory on, from `Mems_allowed_list` in
/// `/proc/self/status`; `None` when it is missing.
#[must_use]
pub fn allowed(root: &Root) -> Option<Vec<usize>> {
    crate::status_list(root, "Mems_allowed_list")
}
