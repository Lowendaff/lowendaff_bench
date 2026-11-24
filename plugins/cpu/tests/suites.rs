//! Parser, runner, and `cpu.bench` selection/precondition tests across all new CPU sub-engines:
//! `7zip`, `unixbench`, `hpl`, `passmark`, `ycruncher`, and `prime95`.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Net;
use bench_api::Plugin;
use bench_host::glibc::GlibcVersion;
use bench_host::glibc::Libc;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use bench_tool::job;
use bench_tool::job::RunError;
use plugin_cpu::bench::CpuBench;
use plugin_cpu::bench::SuiteOutcome;
use plugin_cpu::bench::SuiteSkip;
use plugin_cpu::hpl;
use plugin_cpu::passmark;
use plugin_cpu::prime95;
use plugin_cpu::seven_zip;
use plugin_cpu::unixbench;
use plugin_cpu::ycruncher;
use tokio::time::Instant;

fn fixture(path: &str) -> String {
    let full = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path);
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("read {}: {e}", full.display()))
}

fn exited(code: i32, text: String) -> job::Output {
    job::Output {
        code: Some(code),
        text,
    }
}

#[test]
fn seven_zip_parses_avr_and_tot_mips_and_rejects_missing_tables() {
    let out = exited(0, fixture("7zip/aarch64-2-threads.txt"));
    let parsed = seven_zip::parse::reading(&out).expect("valid 7zip output");
    assert_eq!(parsed.version.as_deref(), Some("26.06"));
    assert_eq!(parsed.compress_kib_s, 27_858);
    assert_eq!(parsed.compress_mips, 24_818);
    assert_eq!(parsed.decompress_kib_s, 139_872);
    assert_eq!(parsed.decompress_mips, 11_311);
    assert_eq!(parsed.usage_pct, 172);
    assert_eq!(parsed.ru_mips, 11_353);
    assert_eq!(parsed.total_mips, 18_065);

    assert_eq!(
        seven_zip::parse::reading(&exited(0, "7-Zip 26.06\n".to_owned())),
        Err(seven_zip::Fault::NoScores)
    );
}

#[test]
fn unixbench_parses_last_count_and_computes_exact_geometric_system_index() {
    let whets = exited(0, fixture("unixbench/whetstone-double.txt"));
    let r_whets = unixbench::parse::copy_reading("whetstone-double", &whets, 10.1).unwrap();
    assert!((r_whets.count - 12_197.144).abs() < 1e-6);
    assert_eq!(r_whets.timebase, 0.0);
    assert_eq!(r_whets.reported_secs, Some(10.08));

    // fstime -c prints Write, Read, Copy COUNT| lines; Copy (the last one, 1667507) must win.
    let fs = exited(0, fixture("unixbench/fstime-copy.txt"));
    let r_fs = unixbench::parse::copy_reading("fstime", &fs, 5.0).unwrap();
    assert_eq!(r_fs.count, 1_667_507.0);
    assert_eq!(r_fs.timebase, 0.0);
    assert_eq!(
        unixbench::parse::copy_reading(
            "fstime",
            &exited(0, "COUNT|500000|0|KBps\nCOUNT|0|0|KBps\n".to_owned()),
            5.0,
        ),
        Err(unixbench::Fault::NoCount { item: "fstime" }),
        "a zero final Copy phase must overwrite earlier Read/Write counts"
    );
    let r_time_reset = unixbench::parse::copy_reading(
        "dhry2reg",
        &exited(0, "TIME|10.0\nCOUNT|1000|1|lps\nTIME|0.0\n".to_owned()),
        f64::INFINITY,
    )
    .unwrap();
    assert_eq!(r_time_reset.reported_secs, None);
    assert!(r_time_reset.elapsed_secs.is_finite());
    assert!(unixbench::parse::combine_copies("shell8", "lpm", f64::NAN, &[r_fs]).is_none());
    assert!(
        unixbench::parse::combine_copies(
            "shell8",
            "lpm",
            6.0,
            &[unixbench::parse::CopyReading {
                count: 1.5e308,
                timebase: 0.0,
                reported_secs: None,
                elapsed_secs: 1.0,
            }],
        )
        .is_none(),
        "overflowing index must return None"
    );

    // looper prints COUNT|<iter>|60|lpm without TIME|; rate = sum(iter) / (elapsed / 60).
    let sh = exited(0, "COUNT|84.8|60|lpm\n".to_owned());
    let r_sh = unixbench::parse::copy_reading("shell1", &sh, 60.0).unwrap();
    let item = unixbench::parse::combine_copies("shell1", "lpm", 42.4, &[r_sh, r_sh]).unwrap();
    assert!((item.rate - 169.6).abs() < 1e-6);
    assert!((item.index - 40.0).abs() < 1e-6);

    let idx = unixbench::parse::geometric_index(&[item]).unwrap();
    assert!((idx - 40.0).abs() < 1e-6);
}

#[test]
fn hpl_parses_passed_gflops_and_rejects_failed_residuals() {
    let out = exited(0, fixture("hpl/aarch64-passed.txt"));
    let parsed = hpl::parse::reading(&out, 4).expect("valid HPL output");
    assert_eq!(parsed.version.as_deref(), Some("2.3"));
    assert_eq!(parsed.threads, 4);
    assert_eq!(parsed.n, 512);
    assert_eq!(parsed.nb, 64);
    assert!((parsed.gflops - 26.81).abs() < 1e-6);
    assert!(parsed.residual.unwrap() < 0.01);

    let bad = fixture("hpl/aarch64-passed.txt").replace("...... PASSED", "...... FAILED");
    assert_eq!(
        hpl::parse::reading(&exited(0, bad), 4),
        Err(hpl::Fault::ResidualFailed)
    );

    let order_zero = hpl::measure::choose_order(Some(0));
    let order_small = hpl::measure::choose_order(Some(512 << 20));
    let order_large = hpl::measure::choose_order(Some(32 << 30));
    assert_eq!(order_zero, hpl::measure::MIN_ORDER);
    assert_eq!(order_small % hpl::measure::BLOCK_SIZE, 0);
    assert_eq!(order_large, hpl::measure::MAX_ORDER);
    assert_eq!(
        hpl::parse::reading(&exited(0, String::new()), 4),
        Err(hpl::Fault::NoResult)
    );
}

#[test]
fn passmark_parses_results_cpu_yml_and_detects_missing_host_lib() {
    let yml = fixture("passmark/aarch64-results_cpu.yml");
    let parsed = passmark::parse::results_yml(&yml).expect("valid PassMark YAML");
    assert_eq!(parsed.version.as_deref(), Some("11.0.1004"));
    assert!((parsed.cpu_mark - 10_413.786_88).abs() < 0.01);
    assert!((parsed.single_thread - 4_324.912_2).abs() < 0.01);
    assert!((parsed.integer_math - 51_407.408).abs() < 0.01);

    let missing = exited(
        127,
        "./PerformanceTest_Linux_x86-64: error while loading shared libraries: \
         libstdc++.so.6: cannot open shared object file: No such file or directory\n"
            .to_owned(),
    );
    assert_eq!(
        passmark::parse::check_exit(&missing),
        Err(passmark::Fault::MissingHostLib("libstdc++.so.6".to_owned()))
    );
}

#[test]
fn ycruncher_parses_pi_times_efficiency_and_enforces_spot_check() {
    let out = exited(0, fixture("ycruncher/x86_64-25m.txt"));
    let parsed = ycruncher::parse::reading(&out, 25_000_000).expect("valid y-cruncher output");
    assert_eq!(parsed.version.as_deref(), Some("0.8.7.9547"));
    assert_eq!(parsed.digits, 25_000_000);
    assert!((parsed.computation_secs - 3.153).abs() < 1e-6);
    assert!((parsed.wall_secs - 3.342).abs() < 1e-6);
    assert!((parsed.cpu_utilization_pct.unwrap() - 724.29).abs() < 1e-2);
    assert!((parsed.multicore_efficiency_pct.unwrap() - 72.43).abs() < 1e-2);
    assert!(parsed.tuning.as_deref().unwrap().contains("13-HSW"));

    let bad = fixture("ycruncher/x86_64-25m.txt").replace("Spot Check: Good", "Spot Check: Failed");
    assert_eq!(
        ycruncher::parse::reading(&exited(0, bad), 25_000_000),
        Err(ycruncher::Fault::ValidationFailed)
    );

    let sgr0_wrapped = fixture("ycruncher/x86_64-25m.txt").replace(
        "Spot Check: Good",
        "\x1b(B\x1b[mSpot Check: Good\x1b]0;unclosed",
    );
    assert!(ycruncher::parse::reading(&exited(0, sgr0_wrapped), 25_000_000).is_ok());
}

#[test]
fn prime95_parses_torture_summary_and_catches_hardware_rounding_errors() {
    let out = job::Output {
        code: None, // ended cleanly on SIGINT after printing torture summaries
        text: fixture("prime95/x86_64-torture.txt"),
    };
    let parsed = prime95::parse::reading(&out, 65).expect("valid Prime95 torture output");
    assert_eq!(parsed.version.as_deref(), Some("30.19b20"));
    assert_eq!(parsed.workers, 4);
    assert_eq!(parsed.duration_secs, 65);
    assert_eq!(parsed.fft_desc.as_deref(), Some("FMA3 FFT length 18K"));
    assert_eq!(parsed.tests_completed, 8);
    assert_eq!(parsed.errors, 0);
    assert_eq!(parsed.warnings, 0);
    assert!(parsed.passed);

    let hw_fail = format!(
        "[Worker #2 Nov 20 18:56] FATAL ERROR: Rounding was 0.5, expected less than 0.4\n{}",
        fixture("prime95/x86_64-torture.txt")
    );
    assert!(matches!(
        prime95::parse::reading(&exited(0, hw_fail), 65),
        Err(prime95::Fault::HardwareError(_))
    ));

    let unprefixed = "FATAL ERROR: Rounding was 0.5 [detail]\n";
    assert_eq!(
        prime95::parse::reading(&exited(0, unprefixed.to_owned()), 65),
        Err(prime95::Fault::HardwareError(
            "FATAL ERROR: Rounding was 0.5 [detail]".to_owned()
        ))
    );
    let rel_job = prime95::measure::attempt(
        Path::new("/tools/prime95/mprime"),
        Path::new("rel/prime95"),
        Duration::from_secs(65),
    );
    assert_eq!(
        rel_job.args,
        [
            OsString::from("-t"),
            OsString::from("-d"),
            OsString::from("-W.")
        ]
    );
}

#[tokio::test]
async fn cpu_bench_config_preconditions_and_partial_arm64_skips_work_cleanly() {
    // Unknown suite name is rejected at config deserialization time.
    assert!(serde_json::from_value::<CpuBench>(serde_json::json!({ "suites": ["pts"] })).is_err());

    // Explicit `"suites": []` disables all suites and skips with `NoSuitesSelected`.
    let none_selected: CpuBench =
        serde_json::from_value(serde_json::json!({ "suites": [] })).unwrap();
    assert_eq!(none_selected.enabled(), &[false; 8]);
    let empty_work = tempfile::tempdir().unwrap();
    let empty_ctx = suite_ctx(empty_work.path());
    assert_eq!(
        none_selected.run(&empty_ctx).await,
        Err(bench_api::Stop::Skipped(
            plugin_cpu::bench::Failure::NoSuitesSelected
        ))
    );

    // Sole `geekbench` selection enforces glibc >= 2.35 and Net::Ipv4 at preflight.
    let gb_only: CpuBench =
        serde_json::from_value(serde_json::json!({ "suites": ["geekbench"] })).unwrap();
    let host_arm = HostProfile {
        arch: "aarch64".to_owned(),
        libc: Libc::Glibc(GlibcVersion {
            major: 2,
            minor: 39,
        }),
        network: Network {
            ipv4: Family {
                targets: vec![("1.1.1.1:443".parse().unwrap(), Reach::Connected)],
                ..Family::default()
            },
            ..Network::default()
        },
        ..HostProfile::default()
    };
    let gb_req = gb_only.host_requirements(&host_arm);
    assert_eq!(gb_req.net, Net::Ipv4);
    assert_eq!(
        gb_req.glibc,
        Some(GlibcVersion {
            major: 2,
            minor: 35
        })
    );

    // `--cpu-bench all` on aarch64 omits x86_64-only tools from `tools_for` and skips them in-run.
    let all_suites: CpuBench =
        serde_json::from_value(serde_json::json!({ "suites": ["all"] })).unwrap();
    let arm_tools: Vec<&str> = all_suites
        .tools_for(&host_arm)
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert!(!arm_tools.contains(&"ycruncher"));
    assert!(!arm_tools.contains(&"prime95"));
    assert!(arm_tools.contains(&"sysbench"));
    assert!(arm_tools.contains(&"passmark"));

    // Running only `ycruncher` + `prime95` on aarch64 skips cleanly without downloading tools.
    let x86_only: CpuBench = serde_json::from_value(serde_json::json!({
        "suites": ["ycruncher", "prime95"]
    }))
    .unwrap();
    assert!(x86_only.tools_for(&host_arm).is_empty());

    let work = tempfile::tempdir().unwrap();
    let (ctx, mut channels) = Context::new(
        Arc::new(host_arm),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(60),
        work.path().to_path_buf(),
        CancellationToken::new(),
    );
    let res = x86_only.run(&ctx).await;
    assert!(matches!(
        res,
        Err(bench_api::Stop::Skipped(
            plugin_cpu::bench::Failure::AllSkipped
        ))
    ));
    let published = channels.published.borrow_and_update().clone().unwrap();
    assert_eq!(
        published.ycruncher,
        Some(SuiteOutcome::Skipped {
            reason: SuiteSkip::ArchUnsupported("aarch64".to_owned()),
        })
    );
    assert_eq!(
        published.prime95,
        Some(SuiteOutcome::Skipped {
            reason: SuiteSkip::ArchUnsupported("aarch64".to_owned()),
        })
    );
}

fn suite_ctx(work: &Path) -> Context<plugin_cpu::bench::Output> {
    let (ctx, _channels) = Context::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(600),
        work.to_path_buf(),
        CancellationToken::new(),
    );
    ctx
}

#[tokio::test]
async fn seven_zip_attempt_and_measure_cover_retries_partial_publish_and_cancel() {
    let program = Path::new("/tools/7zz");
    let low_mem_job = seven_zip::measure::attempt(program, 1, Some(18));
    assert_eq!(low_mem_job.program, program);
    assert_eq!(
        low_mem_job.args,
        [
            OsString::from("b"),
            OsString::from("-mmt1"),
            OsString::from("-md18"),
        ]
    );
    assert_eq!(low_mem_job.limit, seven_zip::measure::ATTEMPT_LIMIT);
    assert!(low_mem_job.group);
    assert!(low_mem_job.first_to_go);

    let full_job = seven_zip::measure::attempt(program, 4, None);
    assert_eq!(
        full_job.args,
        [OsString::from("b"), OsString::from("-mmt4")]
    );

    let work = tempfile::tempdir().unwrap();
    let ctx = suite_ctx(work.path());
    let timed_out = job::Fault::TimedOut(seven_zip::measure::ATTEMPT_LIMIT);
    let ok_text = fixture("7zip/aarch64-2-threads.txt");
    let script = Mutex::new(VecDeque::from([
        Err(RunError::Fault(timed_out.clone())),
        Ok(exited(0, ok_text.clone())),
        Ok(exited(0, ok_text.clone())),
    ]));
    let jobs = Mutex::new(Vec::new());
    let mut partials = Vec::new();
    let out = seven_zip::measure::measure(
        &ctx,
        program,
        4,
        Some(512 << 20),
        |partial| partials.push(partial.clone()),
        |job| {
            jobs.lock().unwrap().push(job);
            let next = script.lock().unwrap().pop_front().unwrap();
            async move { next }
        },
    )
    .await
    .expect("single retry then multi succeeds");

    assert_eq!(
        jobs.into_inner().unwrap(),
        [
            seven_zip::measure::attempt(program, 1, Some(18)),
            seven_zip::measure::attempt(program, 1, Some(18)),
            seven_zip::measure::attempt(program, 4, Some(18)),
        ]
    );
    assert_eq!(out.single.retries, [seven_zip::Fault::Tool(timed_out)]);
    assert_eq!(partials.len(), 1);
    assert!(partials[0].multi.is_none());
    assert_eq!(out.multi.as_ref().unwrap().threads, 4);

    let cancel_script = Mutex::new(VecDeque::from([
        Ok(exited(0, ok_text)),
        Err(RunError::Cancelled),
    ]));
    let cancelled = seven_zip::measure::measure(
        &ctx,
        program,
        4,
        None,
        |_| {},
        |_| {
            let next = cancel_script.lock().unwrap().pop_front().unwrap();
            async move { next }
        },
    )
    .await
    .expect_err("multi-thread cancellation fails with partial");
    assert_eq!(cancelled.1, seven_zip::Failure::Cancelled);
    assert_eq!(cancelled.0.expect("single run kept").single.threads, 1);
}

#[tokio::test]
async fn unixbench_copy_job_and_measure_run_twelve_specs_and_clean_up_work_dir() {
    let bin_dir = Path::new("/tools/unixbench");
    let copy_dir = Path::new("/run/work/unixbench/copies-1/c0");

    let timed =
        unixbench::measure::copy_job(&unixbench::measure::INDEX_SPECS[0], bin_dir, copy_dir);
    assert_eq!(timed.program, bin_dir.join("dhry2reg"));
    assert_eq!(timed.args, [OsString::from("10")]);
    assert_eq!(
        timed.envs,
        [(OsString::from("UB_BINDIR"), OsString::from(bin_dir))]
    );
    assert_eq!(timed.dir.as_deref(), Some(copy_dir));
    assert_eq!(timed.limit, unixbench::measure::ITEM_LIMIT);
    assert!(timed.group);
    assert!(timed.first_to_go);

    let whets =
        unixbench::measure::copy_job(&unixbench::measure::INDEX_SPECS[1], bin_dir, copy_dir);
    assert_eq!(whets.program, bin_dir.join("whetstone-double"));
    assert!(whets.args.is_empty());

    let fscopy =
        unixbench::measure::copy_job(&unixbench::measure::INDEX_SPECS[3], bin_dir, copy_dir);
    assert_eq!(fscopy.program, bin_dir.join("fstime"));
    assert_eq!(
        fscopy.args,
        [
            OsString::from("-c"),
            OsString::from("-t"),
            OsString::from("5"),
            OsString::from("-d"),
            OsString::from(copy_dir),
            OsString::from("-b"),
            OsString::from("1024"),
            OsString::from("-m"),
            OsString::from("2000"),
        ]
    );

    let shell =
        unixbench::measure::copy_job(&unixbench::measure::INDEX_SPECS[9], bin_dir, copy_dir);
    assert_eq!(shell.program, bin_dir.join("looper"));
    assert_eq!(
        shell.args,
        [
            OsString::from("15"),
            bin_dir.join("multi.sh").into_os_string(),
            OsString::from("1"),
        ]
    );

    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("bin");
    std::fs::create_dir_all(&fake_bin).unwrap();
    std::fs::write(fake_bin.join("sort.src"), "alpha\nbeta\n").unwrap();
    let dhry = fake_bin.join("dhry2reg");
    let work_dir = dir.path().join("work");
    let ctx = suite_ctx(dir.path());
    let calls = AtomicUsize::new(0);
    let mut partials = 0_usize;
    let out = unixbench::measure::measure(
        &ctx,
        &dhry,
        &work_dir,
        2,
        |_| partials += 1,
        |job| {
            calls.fetch_add(1, Ordering::Relaxed);
            let copied = job.dir.as_ref().unwrap().join("sort.src");
            assert!(copied.exists(), "sort.src copied into {}", copied.display());
            async { Ok(exited(0, "COUNT|1000|0|lps\nTIME|10.0\n".to_owned())) }
        },
    )
    .await
    .expect("1-copy and 2-copy UnixBench passes succeed");

    assert_eq!(calls.load(Ordering::Relaxed), 12 + 24);
    assert_eq!(partials, 1);
    assert_eq!(out.single.items.len(), 12);
    assert_eq!(out.multi.as_ref().unwrap().copies, 2);
    assert!(!work_dir.exists(), "work_dir removed after completion");

    let cancel_work = dir.path().join("cancel-work");
    let cancelled = unixbench::measure::measure(
        &ctx,
        &dhry,
        &cancel_work,
        1,
        |_| {},
        |_| async { Err(RunError::Cancelled) },
    )
    .await
    .expect_err("cancelled UnixBench fails cleanly");
    assert_eq!(cancelled.1, unixbench::Failure::Cancelled);
    assert!(!cancel_work.exists(), "work_dir removed after cancellation");
}

#[tokio::test]
async fn hpl_attempt_and_measure_write_hpl_dat_and_clean_up_work_dir() {
    let program = Path::new("/tools/xhpl");
    let dir = tempfile::tempdir().unwrap();
    let work_dir = dir.path().join("hpl");
    let expected = hpl::measure::attempt(program, &work_dir, 4);
    assert_eq!(expected.program, program);
    assert!(expected.args.is_empty());
    assert_eq!(
        expected.envs,
        [
            (OsString::from("OMP_NUM_THREADS"), OsString::from("4")),
            (OsString::from("OPENBLAS_NUM_THREADS"), OsString::from("4")),
        ]
    );
    assert_eq!(expected.dir.as_deref(), Some(work_dir.as_path()));
    assert_eq!(expected.limit, hpl::measure::RUN_LIMIT);
    assert!(expected.group);
    assert!(expected.first_to_go);

    let ctx = suite_ctx(dir.path());
    let text = fixture("hpl/aarch64-passed.txt");
    let work_check = work_dir.clone();
    let expected_check = expected.clone();
    let out = hpl::measure::measure(&ctx, program, &work_dir, 4, Some(512 << 20), move |job| {
        assert_eq!(job, expected_check);
        let dat = std::fs::read_to_string(work_check.join("HPL.dat")).unwrap();
        let order = hpl::measure::choose_order(Some(512 << 20));
        assert!(dat.contains(&format!("{order}          Ns")));
        let text = text.clone();
        async move { Ok(exited(0, text)) }
    })
    .await
    .expect("HPL run succeeds");
    assert_eq!(out.threads, 4);
    assert!(!work_dir.exists(), "work_dir removed after completion");

    let cancel_dir = dir.path().join("hpl-cancel");
    let err = hpl::measure::measure(&ctx, program, &cancel_dir, 2, None, |_| async {
        Err(RunError::Cancelled)
    })
    .await
    .expect_err("cancelled HPL fails");
    assert_eq!(err, hpl::Failure::Cancelled);
    assert!(!cancel_dir.exists(), "work_dir removed after cancellation");
}

#[tokio::test]
async fn passmark_attempt_and_measure_read_results_yml_reject_oversize_and_clean_up() {
    let program = Path::new("/tools/pt_linux");
    let libs_file = Path::new("/tools/passmark-libs/libncurses.so.5");
    let dir = tempfile::tempdir().unwrap();
    let work_dir = dir.path().join("passmark");
    let expected = passmark::measure::attempt(program, libs_file, &work_dir);
    assert_eq!(expected.program, program);
    assert_eq!(expected.args, [OsString::from("-r"), OsString::from("1")]);
    assert_eq!(
        expected.envs,
        [
            (
                OsString::from("LD_LIBRARY_PATH"),
                OsString::from("/tools/passmark-libs")
            ),
            (
                OsString::from("TERMINFO"),
                OsString::from("/tools/passmark-libs/terminfo")
            ),
            (OsString::from("TERM"), OsString::from("xterm")),
        ]
    );
    assert_eq!(expected.dir.as_deref(), Some(work_dir.as_path()));
    assert_eq!(expected.limit, passmark::measure::RUN_LIMIT);
    assert!(expected.group);
    assert!(expected.first_to_go);
    assert!(!expected.quotable);

    let ctx = suite_ctx(dir.path());
    let yml = fixture("passmark/aarch64-results_cpu.yml");
    let work_ok = work_dir.clone();
    let expected_check = expected.clone();
    let out = passmark::measure::measure(&ctx, program, libs_file, &work_dir, move |job| {
        assert_eq!(job, expected_check);
        std::fs::write(work_ok.join("results_cpu.yml"), &yml).unwrap();
        async { Ok(exited(0, String::new())) }
    })
    .await
    .expect("PassMark reads results_cpu.yml");
    assert_eq!(out.version.as_deref(), Some("11.0.1004"));
    assert!(!work_dir.exists(), "work_dir removed after completion");

    let big_dir = dir.path().join("passmark-oversize");
    let big_write = big_dir.clone();
    let err = passmark::measure::measure(&ctx, program, libs_file, &big_dir, move |_| {
        let oversized = vec![b'a'; 256 * 1024 + 1];
        std::fs::write(big_write.join("results_cpu.yml"), oversized).unwrap();
        async { Ok(exited(0, String::new())) }
    })
    .await
    .expect_err("oversized results_cpu.yml is rejected");
    assert_eq!(err, passmark::Failure::Run(passmark::Fault::NoScores));
    assert!(!big_dir.exists(), "work_dir removed after oversize error");
}

#[tokio::test]
async fn ycruncher_choose_size_attempt_and_measure_clean_up_work_dir() {
    assert_eq!(
        ycruncher::measure::choose_size(1, None),
        ("25m", 25_000_000)
    );
    assert_eq!(
        ycruncher::measure::choose_size(4, Some((1536 << 20) - 1)),
        ("25m", 25_000_000)
    );
    assert_eq!(
        ycruncher::measure::choose_size(2, Some(1536 << 20)),
        ("100m", 100_000_000)
    );
    assert_eq!(
        ycruncher::measure::choose_size(2, None),
        ("25m", 25_000_000),
        "unknown RAM falls back conservatively to 25m"
    );

    let ansi_out = exited(
        0,
        concat!(
            "\x1b[0;1;33my-cruncher v0.8.7\x1b[0m\n",
            "Program: \x1b[0;1;36my-cruncher v0.8.7 Build 9547\x1b[0m\n",
            "Tuning Profile: \x1b[0;1;32m14-ZN4\u{202e} ~ Ushio\x1b[0m\n",
            "Decimal Digits: 25,000,000\n",
            "Total Computation Time: 1.250 seconds\n",
            "Start-to-End Wall Time: 1.500 seconds\n",
            "CPU Utilization: 724.29%\n",
            "Multi-core Efficiency: -0.0%\n",
            "\x1b[0;1;32mSpot Check: Good\x1b[0m\n"
        )
        .to_owned(),
    );
    let parsed_ansi = ycruncher::parse::reading(&ansi_out, 25_000_000).expect("ANSI y-cruncher");
    assert_eq!(parsed_ansi.version.as_deref(), Some("0.8.7.9547"));
    assert_eq!(parsed_ansi.tuning.as_deref(), Some("14-ZN4 ~ Ushio"));
    assert_eq!(parsed_ansi.cpu_utilization_pct, Some(724.29));
    assert_eq!(parsed_ansi.multicore_efficiency_pct, Some(0.0));
    assert!(
        ycruncher::parse::reading(
            &exited(
                0,
                concat!(
                    "Total Computation Time: 1.0 seconds\n",
                    "Spot Check: Failed\nSpot Check: Good\n"
                )
                .to_owned(),
            ),
            25_000_000
        )
        .is_err(),
        "failed Spot Check must latch across subsequent lines"
    );

    let program = Path::new("/tools/y-cruncher");
    let dir = tempfile::tempdir().unwrap();
    let work_dir = dir.path().join("ycruncher");
    let expected = ycruncher::measure::attempt(program, &work_dir, "25m");
    assert_eq!(
        expected.args,
        [
            OsString::from("skip-warnings"),
            OsString::from("bench"),
            OsString::from("25m"),
        ]
    );
    assert_eq!(expected.dir.as_deref(), Some(work_dir.as_path()));
    assert_eq!(expected.limit, ycruncher::measure::RUN_LIMIT);
    assert!(expected.group);
    assert!(expected.first_to_go);

    let ctx = suite_ctx(dir.path());
    let text = fixture("ycruncher/x86_64-25m.txt");
    let expected_check = expected.clone();
    let out = ycruncher::measure::measure(&ctx, program, &work_dir, 1, None, move |job| {
        assert_eq!(job, expected_check);
        let text = text.clone();
        async move { Ok(exited(0, text)) }
    })
    .await
    .expect("y-cruncher 25m run succeeds");
    assert_eq!(out.digits, 25_000_000);
    assert!(!work_dir.exists(), "work_dir removed after completion");
}

#[tokio::test]
async fn prime95_attempt_and_measure_write_torture_configs_and_clean_up_work_dir() {
    let program = Path::new("/tools/prime95/mprime");
    let dir = tempfile::tempdir().unwrap();
    let work_dir = dir.path().join("prime95");
    let duration = Duration::from_secs(65);
    let expected = prime95::measure::attempt(program, &work_dir, duration);
    let mut work_arg = OsString::from("-W");
    work_arg.push(work_dir.as_os_str());
    assert_eq!(
        expected.args,
        [OsString::from("-t"), OsString::from("-d"), work_arg]
    );
    assert_eq!(
        expected.envs,
        [(
            OsString::from("LD_LIBRARY_PATH"),
            OsString::from("/tools/prime95"),
        )]
    );
    assert_eq!(expected.dir.as_deref(), Some(work_dir.as_path()));
    assert_eq!(expected.interrupt_after, Some(duration));
    assert_eq!(expected.limit, prime95::measure::RUN_LIMIT);
    assert!(expected.group);
    assert!(expected.first_to_go);

    let ctx = suite_ctx(dir.path());
    let text = fixture("prime95/x86_64-torture.txt");
    let work_check = work_dir.clone();
    let expected_check = expected.clone();
    let out = prime95::measure::measure(&ctx, program, &work_dir, 4, duration, move |job| {
        assert_eq!(job, expected_check);
        for name in ["local.txt", "prime.txt"] {
            let cfg = std::fs::read_to_string(work_check.join(name)).unwrap();
            assert!(cfg.contains("TortureThreads=4"));
        }
        let text = text.clone();
        async move { Ok(job::Output { code: None, text }) }
    })
    .await
    .expect("Prime95 torture run succeeds");
    assert!(out.passed);
    assert!(!work_dir.exists(), "work_dir removed after completion");
}

#[tokio::test]
async fn seven_zip_measure_caps_dictionary_records_retries_and_preserves_partial() {
    assert_eq!(seven_zip::measure::choose_dict_exp(4, None), None);
    assert_eq!(
        seven_zip::measure::choose_dict_exp(4, Some(512 << 20)),
        Some(18)
    );
    assert_eq!(
        seven_zip::measure::choose_dict_exp(16, Some(2_u64 << 30)),
        Some(23),
        "2 GiB container on 16 threads caps dictionary well below -md25"
    );
    assert_eq!(
        seven_zip::measure::choose_dict_exp(4, Some(16_u64 << 30)),
        None,
        "roomy host leaves -md25 default uncapped"
    );
    assert_eq!(
        seven_zip::measure::args(0, Some(23)),
        [
            OsString::from("b"),
            OsString::from("-mmt1"),
            OsString::from("-md23"),
        ]
    );

    let program = Path::new("/tools/7zz");
    let dir = tempfile::tempdir().unwrap();
    let expected_1 = seven_zip::measure::attempt(program, 1, Some(23));
    assert!(expected_1.group && expected_1.first_to_go);
    let expected_16 = seven_zip::measure::attempt(program, 16, Some(23));

    let ctx = suite_ctx(dir.path());
    let valid = fixture("7zip/aarch64-2-threads.txt");
    let no_ver = valid.replacen("7-Zip 26.06", "7-Zip", 1);
    let queue = Mutex::new(VecDeque::from([
        (expected_1.clone(), Ok(exited(0, no_ver))),
        (
            expected_16.clone(),
            Ok(exited(0, "7-Zip 26.06\n".to_owned())),
        ),
        (expected_16.clone(), Ok(exited(0, valid.clone()))),
    ]));
    let partials = AtomicUsize::new(0);
    let out = seven_zip::measure::measure(
        &ctx,
        program,
        16,
        Some(2_u64 << 30),
        |partial| {
            assert_eq!(partial.single.threads, 1);
            assert!(partial.multi.is_none());
            partials.fetch_add(1, Ordering::SeqCst);
        },
        |job| {
            let (exp, next) = queue.lock().unwrap().pop_front().unwrap();
            assert_eq!(job, exp);
            async move { next }
        },
    )
    .await
    .expect("7-Zip single + retried multi succeeds");
    assert_eq!(partials.load(Ordering::SeqCst), 1);
    assert_eq!(out.version.as_deref(), Some("26.06"));
    let multi = out.multi.expect("multi result");
    assert_eq!(multi.threads, 16);
    assert_eq!(multi.retries, [seven_zip::Fault::NoScores]);

    // Single-thread host skips multi-thread run.
    let single_only = seven_zip::measure::measure(
        &ctx,
        program,
        1,
        Some(512 << 20),
        |_| panic!("threads <= 1 must not emit partial"),
        |job| {
            assert_eq!(job, seven_zip::measure::attempt(program, 1, Some(18)));
            let text = valid.clone();
            async move { Ok(exited(0, text)) }
        },
    )
    .await
    .expect("single-only succeeds");
    assert!(single_only.multi.is_none());

    // Multi-thread failure preserves single-thread partial output.
    let fail_queue = Mutex::new(VecDeque::from([
        Ok(exited(0, valid)),
        Err(RunError::Cancelled),
    ]));
    let (partial, failure) = seven_zip::measure::measure(
        &ctx,
        program,
        4,
        None,
        |_| {},
        |_| {
            let next = fail_queue.lock().unwrap().pop_front().unwrap();
            async move { next }
        },
    )
    .await
    .expect_err("cancelled multi returns partial");
    assert_eq!(failure, seven_zip::Failure::Cancelled);
    assert_eq!(partial.expect("partial").single.threads, 1);
}

#[tokio::test]
async fn unixbench_copy_job_and_measure_normalize_paths_and_clean_up_work_dir() {
    let fstime_spec = &unixbench::measure::INDEX_SPECS[3];
    let rel_job =
        unixbench::measure::copy_job(fstime_spec, Path::new(""), Path::new("copies-1/c0"));
    assert_eq!(rel_job.program, Path::new("./fstime"));
    assert_eq!(
        rel_job.envs,
        [(OsString::from("UB_BINDIR"), OsString::from("."))]
    );
    assert_eq!(rel_job.dir.as_deref(), Some(Path::new("copies-1/c0")));
    assert_eq!(
        rel_job.args[3..=4],
        [OsString::from("-d"), OsString::from(".")],
        "relative copy_dir must not pass relative path to -d after chdir"
    );

    let dir = tempfile::tempdir().unwrap();
    let work_dir = dir.path().join("unixbench");
    let dhry = dir.path().join("tools/dhry2reg");
    let ctx = suite_ctx(dir.path());
    let partials = AtomicUsize::new(0);
    let out = unixbench::measure::measure(
        &ctx,
        &dhry,
        &work_dir,
        2,
        |partial| {
            assert_eq!(partial.single.copies, 1);
            partials.fetch_add(1, Ordering::SeqCst);
        },
        |job| {
            assert!(job.program.is_absolute());
            assert!(job.dir.as_deref().is_some_and(Path::is_absolute));
            let text = if job
                .program
                .file_name()
                .is_some_and(|n| n == "whetstone-double")
            {
                "COUNT|550.0|1|MWIPS\nTIME|10.0\n".to_owned()
            } else if job.program.file_name().is_some_and(|n| n == "looper") {
                "COUNT|600.0|60|lpm\n".to_owned()
            } else if job.program.file_name().is_some_and(|n| n == "fstime") {
                "COUNT|10000.0|0|KBps\n".to_owned()
            } else {
                "COUNT|100000.0|1|lps\nTIME|10.0\n".to_owned()
            };
            async move { Ok(exited(0, text)) }
        },
    )
    .await
    .expect("unixbench 1-copy + 2-copy succeeds");
    assert_eq!(partials.load(Ordering::SeqCst), 1);
    assert_eq!(out.single.items.len(), 12);
    assert_eq!(out.multi.expect("multi").copies, 2);
    assert!(!work_dir.exists(), "work_dir removed after completion");
}
