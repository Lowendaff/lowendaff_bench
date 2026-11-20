//! A tool that needs raw sockets, in a process where the system would not give them: without
//! CAP_NET_RAW in the bounding set, under which it refuses to start the tool, and with
//! `no_new_privs`, under which it ignores file capabilities. A test binary of its own: both stay
//! for the process once set.

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_tool_that_needs_raw_sockets_is_not_downloaded_where_the_system_would_not_give_them() {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use bench::provision;
    use bench::provision::Part;
    use bench::provision::Provisioned;
    use bench_api::CancellationToken;
    use bench_api::ToolAsset;
    use bench_api::ToolForm;
    use bench_api::ToolSpec;
    use rustix::thread::CapabilitySet;

    const TRACER: ToolSpec = ToolSpec {
        name: "tracer",
        version: "1.0",
        all_arches: false,
        raw_sockets: true,
        form: ToolForm::Executable,
        assets: &[ToolAsset {
            arch: "x86_64",
            url: "https://tools.example/tracer",
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            size: 1,
        }],
    };
    let dir = tempfile::tempdir().expect("a directory for the tool");
    let provision = async || -> (Provisioned, usize) {
        let asked = AtomicUsize::new(0);
        let fetch = |_url, _range| {
            asked.fetch_add(1, Ordering::SeqCst);
            async { Ok(Part::Whole(&b"x"[..])) }
        };
        let cancel = CancellationToken::new();
        let provisioned =
            provision::provision_all(&[TRACER], "x86_64", dir.path(), 1, &cancel, fetch).await;
        (provisioned, asked.into_inner())
    };

    // Root only, which may drop a capability from the bounding set.
    if rustix::process::geteuid().is_root() {
        rustix::thread::remove_capability_from_bounding_set(CapabilitySet::NET_RAW)
            .expect("CAP_NET_RAW leaves the bounding set");
        let (provisioned, asked) = provision().await;
        assert_eq!(
            provisioned.failed["tracer"],
            "it needs raw sockets, but CAP_NET_RAW is not in the program's capability bounding \
             set, as in a container that drops it"
        );
        assert_eq!(asked, 0, "not downloaded");
    }

    rustix::thread::set_no_new_privs(true).expect("no_new_privs is set");
    let (provisioned, asked) = provision().await;
    assert_eq!(
        provisioned.failed["tracer"],
        "it needs raw sockets, but the program runs with no_new_privs, which makes the system \
         ignore file capabilities"
    );
    assert_eq!(asked, 0, "not downloaded");
}
