//! NUMA nodes: the CPUs and the memory of each, the nodes this process may allocate on, and
//! the layout that a test of memory between the nodes can use.

use crate::Root;
use crate::cpu;
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

/// The NUMA layout as this process sees it: every node, and the CPUs and nodes it may use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    /// The nodes, by number; empty when the kernel has no NUMA support.
    pub nodes: Vec<Node>,
    /// The CPUs this process may run on, or `None` when unknown.
    pub cpus: Option<Vec<usize>>,
    /// The nodes this process may allocate on, or `None` when unknown.
    pub mems: Option<Vec<usize>>,
}

/// A node that a test can run threads on, with those of its CPUs that this process may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Threads {
    /// The node.
    pub node: usize,
    /// Its CPUs that this process may use, ascending.
    pub cpus: Vec<usize>,
}

impl Layout {
    /// The layout under `root`.
    #[must_use]
    pub fn read(root: &Root) -> Self {
        Self {
            nodes: read(root),
            cpus: cpu::allowed(root),
            mems: allowed(root),
        }
    }

    /// The nodes that a test can run threads on: those with CPUs this process may use. None
    /// when those CPUs are unknown, since a thread could not be placed.
    #[must_use]
    pub fn thread_nodes(&self) -> Vec<Threads> {
        let Some(allowed) = &self.cpus else {
            return Vec::new();
        };
        self.nodes
            .iter()
            .filter_map(|node| {
                let cpus: Vec<usize> = node
                    .cpus
                    .iter()
                    .copied()
                    .filter(|cpu| allowed.contains(cpu))
                    .collect();
                (!cpus.is_empty()).then_some(Threads {
                    node: node.id,
                    cpus,
                })
            })
            .collect()
    }

    /// The nodes that a test can place memory on: those with memory, among the nodes this
    /// process may allocate on when they are known.
    #[must_use]
    pub fn memory_nodes(&self) -> Vec<&Node> {
        self.nodes
            .iter()
            .filter(|node| {
                node.total.is_some_and(|total| total > 0)
                    && self
                        .mems
                        .as_deref()
                        .is_none_or(|mems| mems.contains(&node.id))
            })
            .collect()
    }

    /// How many nodes a test between the nodes can use: those it can run threads on or place
    /// memory on, counted once; none when it can do only one of the two.
    #[must_use]
    pub fn usable(&self) -> usize {
        let threads = self.thread_nodes();
        let memory = self.memory_nodes();
        if threads.is_empty() || memory.is_empty() {
            return 0;
        }
        let mut nodes: Vec<usize> = threads
            .iter()
            .map(|threads| threads.node)
            .chain(memory.iter().map(|node| node.id))
            .collect();
        nodes.sort_unstable();
        nodes.dedup();
        nodes.len()
    }
}
