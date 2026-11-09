//! The kernel's count of processes killed for lack of memory (design cpu, §6.5).

use bench_host::Root;
use plugin_geekbench::process;

#[test]
fn the_oom_kill_count_comes_from_vmstat() {
    let host = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir(host.path().join("proc")).expect("proc");
    let vmstat = host.path().join("proc/vmstat");
    std::fs::write(
        &vmstat,
        "oom_kill_disabled 0\npgfault 99\noom_kill 7\nunevictable 0\n",
    )
    .expect("vmstat");
    let root = Root::new(host.path());
    assert_eq!(process::oom_kills(&root), Some(7));

    std::fs::write(&vmstat, "pgfault 99\n").expect("vmstat");
    assert_eq!(
        process::oom_kills(&root),
        None,
        "kernels before 4.13 do not count"
    );
}
