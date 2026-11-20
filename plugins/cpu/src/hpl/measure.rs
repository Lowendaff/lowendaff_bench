//! Generates `HPL.dat` sized safely to available RAM and runs `xhpl` with OpenBLAS threads.

use std::future::Future;
use std::path::Path;
use std::time::Duration;

use bench_api::Context;
use bench_api::Measurement;
use bench_api::Step;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;

use super::parse;
use super::result::Failure;
use super::result::Fault;
use super::result::Output;

/// Block size `NB` tuned for modern x86_64 AVX2/AVX-512 and ARM64 NEON/SVE caches.
pub const BLOCK_SIZE: u32 = 192;

/// Minimum matrix order (`2304`: ~40.5 MiB for `N * N * 8` bytes).
pub const MIN_ORDER: u32 = 2_304;

/// Maximum matrix order (`11520`: ~1.01 GiB, keeping single-node runs fast across core counts).
pub const MAX_ORDER: u32 = 11_520;

/// Time limit for one `xhpl` solve.
pub const RUN_LIMIT: Duration = Duration::from_secs(10 * 60);

/// Chooses matrix order `N` (a multiple of [`BLOCK_SIZE`]) using at most 25% of available memory,
/// clamped to `[MIN_ORDER, MAX_ORDER]`.
#[must_use]
pub fn choose_order(available_bytes: Option<u64>) -> u32 {
    let budget = available_bytes
        .map_or(256 << 20, |avail| avail / 4)
        .clamp(48 << 20, 1088 << 20);
    let elements = (budget / 8) as f64;
    let raw = elements.sqrt() as u32;
    let blocks = (raw / BLOCK_SIZE).max(1);
    (blocks * BLOCK_SIZE).clamp(MIN_ORDER, MAX_ORDER)
}

/// Formats a single-rank (`P=1, Q=1`) `HPL.dat` input file for matrix order `n` and block `nb`.
#[must_use]
pub fn hpl_dat(n: u32, nb: u32) -> String {
    format!(
        "HPLinpack benchmark input file\n\
         Innovative Computing Laboratory, University of Tennessee\n\
         HPL.out      output file name (if any)\n\
         6            device out (6=stdout,7=stderr,file)\n\
         1            # of problems sizes (N)\n\
         {n}          Ns\n\
         1            # of NBs\n\
         {nb}         NBs\n\
         0            PMAP process mapping (0=Row-,1=Column-major)\n\
         1            # of process grids (P x Q)\n\
         1            Ps\n\
         1            Qs\n\
         16.0         threshold\n\
         1            # of panel fact\n\
         2            PFACTs (0=left, 1=Crout, 2=Right)\n\
         1            # of recursive stopping criterium\n\
         4            NBMINs (>= 1)\n\
         1            # of panels in recursion\n\
         2            NDIVs\n\
         1            # of recursive panel fact.\n\
         1            RFACTs (0=left, 1=Crout, 2=Right)\n\
         1            # of broadcast\n\
         1            BCASTs (0=1rg,1=1rM,2=2rg,3=2rM,4=Lng,5=LnM)\n\
         1            # of lookahead depth\n\
         1            DEPTHs (>=0)\n\
         2            SWAP (0=bin-exch,1=long,2=mix)\n\
         64           swapping threshold\n\
         0            L1 in (0=transposed,1=no-transposed) form\n\
         0            U  in (0=transposed,1=no-transposed) form\n\
         1            Equilibration (0=no,1=yes)\n\
         8            memory alignment in double (> 0)\n"
    )
}

/// Builds the `xhpl` job in `work_dir` with `threads` OpenMP/OpenBLAS threads.
#[must_use]
pub fn attempt(program: &Path, work_dir: &Path, threads: usize) -> Job {
    let mut job = Job::new(program, Vec::<&str>::new(), RUN_LIMIT);
    let count = threads.max(1).to_string();
    job.envs = vec![
        ("OMP_NUM_THREADS".into(), count.clone().into()),
        ("OPENBLAS_NUM_THREADS".into(), count.into()),
    ];
    job.dir = Some(work_dir.to_path_buf());
    job.group = true;
    job.first_to_go = true;
    job
}

/// Writes `HPL.dat` into `work_dir` and executes `xhpl`.
pub async fn measure<O, F, Fut>(
    ctx: &Context<O>,
    program: &Path,
    work_dir: &Path,
    threads: usize,
    available_bytes: Option<u64>,
    run: F,
) -> Result<Output, Failure>
where
    O: Measurement,
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    ctx.progress(Step::CpuSuite {
        suite: "hpl",
        stage: None,
        threads: Some(threads),
    });
    if let Err(err) = tokio::fs::create_dir_all(work_dir).await {
        return Err(Failure::Run(Fault::Setup(err.to_string())));
    }
    let n = choose_order(available_bytes);
    let dat_path = work_dir.join("HPL.dat");
    if let Err(err) = tokio::fs::write(&dat_path, hpl_dat(n, BLOCK_SIZE)).await {
        return Err(Failure::Run(Fault::Setup(err.to_string())));
    }
    match run(attempt(program, work_dir, threads)).await {
        Ok(output) => parse::reading(&output, threads).map_err(Failure::Run),
        Err(RunError::Fault(fault)) => Err(Failure::Run(Fault::Tool(fault))),
        Err(RunError::Cancelled) => Err(Failure::Cancelled),
    }
}
