//! Parser, runner, and `cpu.bench` selection/precondition tests across all new CPU sub-engines:
//! `7zip`, `unixbench`, `hpl`, `passmark`, `ycruncher`, and `prime95`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
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

    let order_small = hpl::measure::choose_order(Some(512 << 20));
    let order_large = hpl::measure::choose_order(Some(32 << 30));
    assert_eq!(order_small % hpl::measure::BLOCK_SIZE, 0);
    assert_eq!(order_large, hpl::measure::MAX_ORDER);
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
}

#[tokio::test]
async fn cpu_bench_config_preconditions_and_partial_arm64_skips_work_cleanly() {
    // Unknown suite name is rejected at config deserialization time.
    assert!(serde_json::from_value::<CpuBench>(serde_json::json!({ "suites": ["pts"] })).is_err());

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
