//! What the memory plugins run, and with how much memory, worked out from the host facts alone:
//! the working-set plan they share. Each plugin reads it when it starts, since the available
//! memory changes.

use std::num::NonZeroUsize;

use bench_host::Root;
use bench_host::cache;
use bench_host::cache::Cache;
use bench_host::cache::CacheKind;
use bench_host::cpu;
use bench_host::memory;
use bench_host::numa::Layout;
use bench_host::numa::Threads;
use bench_host::platform::PlatformKind;
use serde::Serialize;

/// One KiB, in bytes.
pub const KIB: u64 = 1 << 10;

/// One MiB, in bytes.
pub const MIB: u64 = 1 << 20;

/// One GiB, in bytes.
pub const GIB: u64 = 1 << 30;

/// The last-level caches assumed when the kernel does not describe the caches.
pub const UNKNOWN_CACHE: u64 = 32 * MIB;

/// STREAM's sizes, in millions of elements per array.
pub const STREAM_SIZES: [u64; 6] = [2, 4, 8, 16, 32, 64];

/// No buffer is halved below a page.
const PAGE: u64 = 4 * KIB;

/// What the plugins read about the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// Memory a test may use without swapping (A).
    pub available: u64,
    /// The usable threads (N).
    pub threads: usize,
    /// Every cache instance.
    pub caches: Vec<Cache>,
    /// The NUMA nodes, and the CPUs and nodes this process may use.
    pub layout: Layout,
}

impl Host {
    /// Reads the host under `root` on a platform of `kind`; `None` when its available memory
    /// is unknown.
    #[must_use]
    pub fn read(root: &Root, kind: PlatformKind) -> Option<Self> {
        Some(Self {
            available: memory::usable(root, kind)?,
            threads: cpu::usable_threads(root, kind)
                .or_else(|| {
                    std::thread::available_parallelism()
                        .ok()
                        .map(NonZeroUsize::get)
                })
                .unwrap_or(1),
            caches: cache::read(root),
            layout: Layout::read(root),
        })
    }
}

/// A row of the latency table: a cache level, or memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Level {
    /// The level-1 data cache.
    L1,
    /// The level-2 cache.
    L2,
    /// The level-3 cache.
    L3,
    /// Memory.
    Memory,
}

/// A level of the latency table, and the buffer that multichase chases through for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chase {
    /// The level.
    pub level: Level,
    /// The buffer, in bytes.
    pub buffer: u64,
}

/// The buffers of one cell of the NUMA matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// multiload's buffer per thread, in bytes.
    pub per_thread: u64,
    /// multichase's buffer, in bytes.
    pub latency: u64,
}

/// The NUMA matrix: threads on the rows' nodes, memory on the columns' nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matrix {
    /// The nodes that the threads run on, with the CPUs they run on.
    pub rows: Vec<Threads>,
    /// The nodes that the memory is placed on.
    pub columns: Vec<usize>,
    /// The cells, row by row.
    pub cells: Vec<Vec<Cell>>,
}

/// Every run of the memory plugins and its buffers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The threads of the multi-thread runs; with one, there are none.
    pub threads: usize,
    /// The last-level caches' total (C).
    pub cache: u64,
    /// Whether [`cache`](Self::cache) is [`UNKNOWN_CACHE`] for want of cache information.
    pub cache_unknown: bool,
    /// Half the available memory: the most a run uses (B).
    pub budget: u64,
    /// The buffer of single-thread sysbench and of memory's latency.
    pub single: u64,
    /// sysbench's buffer per thread in the multi-thread runs.
    pub per_thread: u64,
    /// STREAM's size in millions of elements per array, or `None` when not even the smallest
    /// fits.
    pub stream: Option<u64>,
    /// The latency table's rows, memory last.
    pub chases: Vec<Chase>,
    /// The cache levels left out of the latency table for want of their size.
    pub missing: Vec<Level>,
    /// The NUMA matrix, or `None` when fewer than two NUMA nodes can be used.
    pub matrix: Option<Matrix>,
}

impl Plan {
    /// The plan for `host`.
    #[must_use]
    pub fn new(host: &Host) -> Self {
        let budget = host.available / 2;
        let threads = host.threads.max(1);
        let known = last_level(&host.caches, None);
        let cache = known.unwrap_or(UNKNOWN_CACHE);
        let single = fit(at_least(cache.saturating_mul(4)), 1, budget);
        let (chases, missing) = chases(&host.caches, single);
        Self {
            threads,
            cache,
            cache_unknown: known.is_none(),
            budget,
            single,
            per_thread: per_thread(cache, threads, budget),
            stream: stream(cache, budget),
            chases,
            missing,
            matrix: matrix(host, cache, budget),
        }
    }

    /// The plan for this host, read on a blocking thread; `None` when its available memory is
    /// unknown.
    pub async fn read(kind: PlatformKind) -> Option<Self> {
        tokio::task::spawn_blocking(move || Host::read(&Root::system(), kind))
            .await
            .expect("reading the host does not panic")
            .map(|host| Self::new(&host))
    }

    /// The threads of each column of results: one, and all the usable threads when there are
    /// more.
    #[must_use]
    pub fn columns(&self) -> Vec<usize> {
        if self.threads > 1 {
            vec![1, self.threads]
        } else {
            vec![1]
        }
    }

    /// Four times the last-level caches, when `bytes` is less: data that small partly stays in
    /// the caches.
    #[must_use]
    pub fn below_caches(&self, bytes: u64) -> Option<u64> {
        let four = self.cache.saturating_mul(4);
        (bytes < four).then_some(four)
    }
}

/// Bytes of one STREAM array of `millions` million doubles.
#[must_use]
pub fn array_bytes(millions: u64) -> u64 {
    millions * 8_000_000
}

/// The smallest power of two of at least `bytes`.
fn at_least(bytes: u64) -> u64 {
    bytes.checked_next_power_of_two().unwrap_or(1 << 63)
}

/// `start`, halved until `count` of it fit in `budget`, but not below a page.
fn fit(start: u64, count: u64, budget: u64) -> u64 {
    let mut size = start;
    while size > PAGE && size.saturating_mul(count) > budget {
        size /= 2;
    }
    size
}

/// The buffer of each of `threads` threads: the smallest power of two of at least
/// 4 × `cache` ÷ `threads` and at least 1 MiB, halved until all of them fit in `budget`.
fn per_thread(cache: u64, threads: usize, budget: u64) -> u64 {
    let threads = threads as u64;
    let start = at_least(cache.saturating_mul(4).div_ceil(threads)).max(MIB);
    fit(start, threads, budget)
}

/// The total of the highest level's instances, counting only those that serve `cpus` when
/// given; `None` when there is none.
fn last_level(caches: &[Cache], cpus: Option<&[usize]>) -> Option<u64> {
    let holds_data = |cache: &&Cache| cache.kind != CacheKind::Instruction;
    let level = caches
        .iter()
        .filter(holds_data)
        .map(|cache| cache.level)
        .max()?;
    let total: u64 = caches
        .iter()
        .filter(holds_data)
        .filter(|cache| cache.level == level)
        .filter(|cache| cpus.is_none_or(|cpus| cache.cpus.iter().any(|cpu| cpus.contains(cpu))))
        .map(|cache| cache.size)
        .sum();
    (total > 0).then_some(total)
}

/// The latency rows: half of one instance of L1d, L2 and L3, then memory with `memory`
/// bytes; and the cache levels without a size.
fn chases(caches: &[Cache], memory: u64) -> (Vec<Chase>, Vec<Level>) {
    let mut chases = Vec::new();
    let mut missing = Vec::new();
    for (level, number) in [(Level::L1, 1), (Level::L2, 2), (Level::L3, 3)] {
        let found = caches
            .iter()
            .find(|cache| cache.level == number && cache.kind != CacheKind::Instruction);
        match found {
            Some(cache) => chases.push(Chase {
                level,
                buffer: cache.size / 2,
            }),
            None => missing.push(level),
        }
    }
    chases.push(Chase {
        level: Level::Memory,
        buffer: memory,
    });
    (chases, missing)
}

/// STREAM's size: the smallest whose array is at least 4 × `cache`, else the largest;
/// then the largest no bigger whose three arrays fit in `budget`. `None` when none fits.
fn stream(cache: u64, budget: u64) -> Option<u64> {
    let four = cache.saturating_mul(4);
    let wanted = STREAM_SIZES
        .iter()
        .position(|&millions| array_bytes(millions) >= four)
        .unwrap_or(STREAM_SIZES.len() - 1);
    STREAM_SIZES[..=wanted]
        .iter()
        .rev()
        .copied()
        .find(|&millions| 3 * array_bytes(millions) <= budget)
}

/// The NUMA matrix: a row for each node with CPUs this process may use, a column for each node
/// with memory it may allocate on; `None` when fewer than two nodes can be used.
fn matrix(host: &Host, cache: u64, budget: u64) -> Option<Matrix> {
    if host.layout.usable() < 2 {
        return None;
    }
    let rows = host.layout.thread_nodes();
    let columns = host.layout.memory_nodes();
    let cells = rows
        .iter()
        .map(|row| {
            let near = last_level(&host.caches, Some(&row.cpus)).unwrap_or(cache);
            columns
                .iter()
                .map(|column| {
                    let room = column.free.map_or(budget, |free| budget.min(free / 2));
                    Cell {
                        per_thread: per_thread(near, row.cpus.len(), room),
                        latency: fit(at_least(near.saturating_mul(4)), 1, room),
                    }
                })
                .collect()
        })
        .collect();
    Some(Matrix {
        columns: columns.iter().map(|node| node.id).collect(),
        rows,
        cells,
    })
}
