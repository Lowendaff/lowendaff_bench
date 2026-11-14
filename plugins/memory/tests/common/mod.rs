//! Helpers shared by the tests of the plugins' measurements: recorded runs in place of the tools,
//! the context and what came through it, and made-up hosts.

#![allow(
    dead_code,
    reason = "each test binary uses only the helpers of its plugin"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Channels;
use bench_api::Context;
use bench_api::Measurement;
use bench_api::Progress;
use bench_host::cache::Cache;
use bench_host::cache::CacheKind;
use bench_host::numa::Layout;
use bench_host::numa::Node;
use bench_host::profile::HostProfile;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use plugin_memory::Value;
use plugin_memory::plan::GIB;
use plugin_memory::plan::Host;
use plugin_memory::plan::KIB;
use plugin_memory::plan::MIB;
use tokio::time::Instant;

/// What a tool printed, by the name of a file in `tests/fixtures` (see the README there).
pub fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(&path).expect("fixtures are committed next to the tests")
}

/// What a run that ended with `code` printed: `text`.
pub fn output(code: i32, text: &str) -> job::Output {
    job::Output {
        code: Some(code),
        text: text.to_owned(),
    }
}

/// A run that ended with `code` after printing `text`.
pub fn exited(code: i32, text: &str) -> Result<job::Output, RunError> {
    Ok(output(code, text))
}

/// What a tool printed in a fixture, with an exit code of 0.
pub fn printed(name: &str) -> job::Output {
    output(0, &fixture(name))
}

/// A number as a tool prints it.
pub fn value(text: &str) -> Value {
    Value::parse(text).expect("a number")
}

/// The fault of a run that exited with `code` after printing `last` last.
pub fn exit(code: i32, last: &str) -> job::Fault {
    job::Fault::Exit {
        code,
        last: Some(last.to_owned()),
    }
}

/// Recorded runs: each job is answered by a function of the job and of how many times the same
/// job ran before, and kept.
pub struct Recorder<A> {
    answer: A,
    jobs: Mutex<Vec<Job>>,
}

impl<A> Recorder<A>
where
    A: Fn(&Job, usize) -> Result<job::Output, RunError>,
{
    /// Runs that `answer` answers.
    pub fn new(answer: A) -> Self {
        Self {
            answer,
            jobs: Mutex::new(Vec::new()),
        }
    }

    /// Runs `job`: records it and answers it.
    pub fn run(&self, job: Job) -> std::future::Ready<Result<job::Output, RunError>> {
        let mut jobs = self.jobs.lock().expect("the jobs");
        let before = jobs.iter().filter(|seen| **seen == job).count();
        let answer = (self.answer)(&job, before);
        jobs.push(job);
        std::future::ready(answer)
    }

    /// The jobs run so far, in order.
    pub fn jobs(&self) -> Vec<Job> {
        self.jobs.lock().expect("the jobs").clone()
    }
}

/// A context with time to spare, cancelled by `cancel`, and its channels.
pub fn context<O: Measurement>(cancel: &CancellationToken) -> (Context<O>, Channels<O>) {
    Context::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(3600),
        "/run/work".into(),
        cancel.clone(),
    )
}

/// The last result published through `channels`, and the progress lines.
pub fn drained<O: Measurement>(channels: &mut Channels<O>) -> (Option<O>, Vec<String>) {
    let mut lines = Vec::new();
    while let Ok(item) = channels.progress.try_recv() {
        if let Progress::Line(line) = item {
            lines.push(line);
        }
    }
    (channels.published.borrow().clone(), lines)
}

/// A cache instance.
pub fn cache(level: u32, kind: CacheKind, size: u64, cpus: &[usize]) -> Cache {
    Cache {
        level,
        kind,
        size,
        cpus: cpus.to_vec(),
    }
}

/// Four CPUs with an L1d and an L2 each and one L3 of 8 MiB, 8 GiB available, one node.
pub fn host() -> Host {
    let mut caches: Vec<Cache> = (0..4)
        .flat_map(|cpu| {
            [
                cache(1, CacheKind::Data, 32 * KIB, &[cpu]),
                cache(1, CacheKind::Instruction, 32 * KIB, &[cpu]),
                cache(2, CacheKind::Unified, MIB, &[cpu]),
            ]
        })
        .collect();
    caches.push(cache(3, CacheKind::Unified, 8 * MIB, &[0, 1, 2, 3]));
    Host {
        available: 8 * GIB,
        threads: 4,
        caches,
        layout: Layout {
            nodes: vec![Node {
                id: 0,
                cpus: vec![0, 1, 2, 3],
                total: Some(16 * GIB),
                free: Some(8 * GIB),
            }],
            cpus: Some(vec![0, 1, 2, 3]),
            mems: Some(vec![0]),
        },
    }
}

/// [`host`] with its CPUs on two nodes, each with an L3 of 8 MiB.
pub fn two_nodes() -> Host {
    let mut host = host();
    host.caches.pop();
    host.caches
        .push(cache(3, CacheKind::Unified, 8 * MIB, &[0, 1]));
    host.caches
        .push(cache(3, CacheKind::Unified, 8 * MIB, &[2, 3]));
    host.layout.nodes = (0..2)
        .map(|id| Node {
            id,
            cpus: vec![2 * id, 2 * id + 1],
            total: Some(16 * GIB),
            free: Some(8 * GIB),
        })
        .collect();
    host.layout.mems = Some(vec![0, 1]);
    host
}

/// An argument's value, such as that of `-m`.
pub fn arg<'a>(job: &'a Job, name: &str) -> Option<&'a str> {
    let at = job.args.iter().position(|arg| arg == name)?;
    job.args.get(at + 1).and_then(|value| value.to_str())
}

/// Whether `job` has the argument `arg`.
pub fn has(job: &Job, arg: &str) -> bool {
    job.args.iter().any(|seen| seen == arg)
}

/// The arguments of `job`, as strings.
pub fn args(job: &Job) -> Vec<String> {
    job.args
        .iter()
        .map(|arg| arg.to_str().expect("the arguments are UTF-8").to_owned())
        .collect()
}
