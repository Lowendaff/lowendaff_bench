//! End-to-end state-machine, configuration, input-decoding, and layout tests for `--tui`.

use std::sync::Arc;
use std::time::Duration;

use bench::app;
use bench::app::Outcome;
use bench::orchestrator::Planned;
use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::registry;
use bench::report::Format;
use bench::results::Measured;
use bench::results::Report;
use bench::tui;
use bench::tui::Command;
use bench::tui::ItemParams;
use bench::tui::Key;
use bench::tui::Pane;
use bench::tui::Screen;
use bench::tui::Session;
use bench::tui::TuiConfig;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use plugin_cpu::geekbench;
use plugin_cpu::geekbench::result::Scores;
use unicode_width::UnicodeWidthStr;

fn type_str(session: &mut Session, text: &str) {
    for ch in text.chars() {
        let _ = session.handle_key(Key::Char(ch));
    }
}

fn sample_outcome(exit_code: u8) -> Outcome {
    let ipv4: std::net::IpAddr = [198, 51, 100, 42].into();
    let target = std::net::SocketAddr::from(([198, 51, 100, 1], 443));
    let host = HostProfile {
        network: Network {
            ipv4: Family {
                targets: vec![(target, Reach::Connected)],
                address: Address::Found {
                    address: ipv4,
                    source: "Cloudflare",
                },
            },
            ..Network::default()
        },
        ..HostProfile::default()
    };
    let run = Run {
        id: "cpu.geekbench",
        group: "cpu",
        title: Some("Geekbench 7"),
        started: Some(Duration::from_secs(0)),
        elapsed: Some(Duration::from_millis(1500)),
        status: Status::Completed,
        result: Some(Measured::CpuGeekbench(Report {
            output: Some(geekbench::Output {
                scores: Some(Scores {
                    version: Some("7.1.0".to_owned()),
                    single: 2100,
                    multi: 9400,
                    url: None,
                }),
                ..geekbench::Output::default()
            }),
            stop: None,
        })),
    };
    Outcome {
        host: Arc::new(host),
        not_applicable: vec!["Disk health"],
        runs: vec![run],
        total: Duration::from_secs(2),
        exit_code,
    }
}

#[test]
fn vt100_key_decoder_handles_arrows_modifiers_utf8_and_split_escapes() {
    let raw = b"\x1b[A\x1b[B\x1b[C\x1b[D\x1b[Z\x1b[5~\x1b[6~\
                \x1b[H\x1b[F\x1b[3~\x1b[15~\r\t\x7f\x03\x15q ";
    let (keys, used) = tui::input::parse_keys(raw, false);
    assert_eq!(used, raw.len());
    assert_eq!(
        keys,
        [
            Key::Up,
            Key::Down,
            Key::Right,
            Key::Left,
            Key::BackTab,
            Key::PageUp,
            Key::PageDown,
            Key::Home,
            Key::End,
            Key::Delete,
            Key::F5,
            Key::Enter,
            Key::Tab,
            Key::Backspace,
            Key::CtrlC,
            Key::CtrlU,
            Key::Char('q'),
            Key::Char(' '),
        ]
    );

    // Lone ESC waits when flush_incomplete is false, then resolves to Key::Esc when flushed.
    assert_eq!(tui::input::parse_keys(b"\x1b", false), (vec![], 0));
    assert_eq!(tui::input::parse_keys(b"\x1b", true), (vec![Key::Esc], 1));

    // Partial CSI sequence waits until the final byte arrives.
    assert_eq!(tui::input::parse_keys(b"\x1b[15", false), (vec![], 0));
    assert_eq!(
        tui::input::parse_keys(b"\x1b[15~", false),
        (vec![Key::F5], 5)
    );
}

#[test]
fn defaults_match_standard_cli_and_every_customized_plugin_builds_cleanly() {
    let all = registry::all();
    let mut cfg = TuiConfig::from_registry(&all);

    assert_eq!(cfg.items.len(), 13);
    assert_eq!(cfg.enabled_count(), 13);
    assert_eq!(cfg.customized_count(), 0);
    assert!(
        cfg.plugin_settings().is_empty(),
        "defaults emit no overrides"
    );
    assert_eq!(cfg.global.format, Format::Text);
    assert_eq!(cfg.global.output, "");
    assert_eq!(cfg.global.work_dir, ".");
    assert!(!cfg.global.show_ip, "public IP must be masked by default");
    assert_eq!(cfg.global.download_connections, app::DOWNLOAD_CONNECTIONS);
    assert!(!cfg.global.verbose);

    let iperf_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.iperf3")
        .unwrap();
    cfg.apply_item_edit(iperf_idx, 1, " US , JP ").unwrap();
    cfg.apply_item_edit(iperf_idx, 2, "tokyo-1").unwrap();
    cfg.apply_item_edit(iperf_idx, 3, "15").unwrap();
    cfg.apply_item_edit(iperf_idx, 4, "4").unwrap();
    cfg.apply_item_edit(iperf_idx, 5, "500").unwrap();
    assert!(
        cfg.apply_item_edit(iperf_idx, 3, "0").is_err(),
        "0 seconds must be rejected"
    );
    assert!(
        cfg.apply_item_edit(iperf_idx, 4, "200").is_err(),
        ">128 streams must be rejected"
    );

    let trace_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.trace")
        .unwrap();
    assert_eq!(cfg.activate_item_field(trace_idx, 2), None); // china
    assert_eq!(cfg.activate_item_field(trace_idx, 6), None); // geo -> false
    cfg.apply_item_edit(trace_idx, 5, "youtube,ct-sh").unwrap();

    let quality_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "ip.quality")
        .unwrap();
    assert_eq!(cfg.activate_item_field(quality_idx, 1), None); // dnsbl -> false
    assert_eq!(cfg.activate_item_field(quality_idx, 2), None); // mail -> false

    let media_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "ip.media")
        .unwrap();
    assert_eq!(cfg.activate_item_field(media_idx, 1), None); // global
    assert_eq!(cfg.activate_item_field(media_idx, 2), None); // asia
    cfg.apply_item_edit(media_idx, 7, "netflix,youtube")
        .unwrap();

    assert_eq!(cfg.customized_count(), 4);
    let settings = cfg.plugin_settings();
    assert_eq!(
        settings["network.iperf3"],
        serde_json::json!({
            "countries": ["US", "JP"],
            "servers": ["tokyo-1"],
            "seconds": 15,
            "streams": 4,
            "rate": 500,
        })
    );
    assert_eq!(
        settings["network.trace"],
        serde_json::json!({
            "groups": ["china"],
            "targets": ["youtube", "ct-sh"],
            "geo": false,
        })
    );
    assert_eq!(
        settings["ip.quality"],
        serde_json::json!({
            "dnsbl": false,
            "mail": false,
        })
    );
    assert_eq!(
        settings["ip.media"],
        serde_json::json!({
            "groups": ["global", "asia"],
            "services": ["netflix", "youtube"],
        })
    );

    // Every customized plugin config must pass serde + orchestrator::plan validation!
    let planned = cfg
        .build_plan(&all)
        .expect("customized settings must build");
    assert_eq!(planned.len(), 13);

    // Disabling every item must produce a friendly validation error instead of an empty run.
    cfg.toggle_all();
    assert_eq!(cfg.enabled_count(), 0);
    assert!(cfg.build_plan(&all).is_err());
}

#[test]
fn keyboard_navigation_drives_item_switches_groups_params_and_global_options() {
    let all = registry::all();
    let mut session = Session::new(&all);
    assert_eq!(session.screen, Screen::Configure);
    assert_eq!(session.focus, Pane::Left);

    // Move from Start (0) to disk.health (1) and toggle off with Space.
    assert_eq!(session.handle_key(Key::Down), Command::None);
    assert_eq!(session.handle_key(Key::Char(' ')), Command::None);
    assert!(!session.config.items[0].enabled);

    // Move to memory.sysbench (3) and press 'g' to toggle the entire memory group off.
    let _ = session.handle_key(Key::Down); // cpu.sysbench (2)
    let _ = session.handle_key(Key::Down); // memory.sysbench (3)
    let _ = session.handle_key(Key::Char('g'));
    for item in session.config.items.iter().filter(|i| i.group == "memory") {
        assert!(
            !item.enabled,
            "{} should be disabled by group toggle",
            item.id
        );
    }

    // Jump to Global options at End, focus right pane, cycle format, and edit connections.
    let _ = session.handle_key(Key::End);
    let _ = session.handle_key(Key::Enter);
    assert_eq!(session.focus, Pane::Right);
    let _ = session.handle_key(Key::Right); // format: Text -> Markdown
    assert_eq!(session.config.global.format, Format::Markdown);

    // Move down to download_connections (field 4), open inline editor, enter invalid then valid.
    for _ in 0..4 {
        let _ = session.handle_key(Key::Down);
    }
    let _ = session.handle_key(Key::Enter);
    assert!(session.editor.is_some());
    let _ = session.handle_key(Key::CtrlU);
    type_str(&mut session, "999");
    let _ = session.handle_key(Key::Enter);
    assert!(
        session.editor.is_some(),
        "invalid connection count keeps editor open with error"
    );
    let _ = session.handle_key(Key::CtrlU);
    type_str(&mut session, "16");
    let _ = session.handle_key(Key::Enter);
    assert!(session.editor.is_none());
    assert_eq!(session.config.global.download_connections, 16);

    // Esc returns focus to Left list; 'r' triggers StartRun; 'd' on Start resets everything.
    let _ = session.handle_key(Key::Esc);
    assert_eq!(session.focus, Pane::Left);
    assert_eq!(session.handle_key(Key::Char('r')), Command::StartRun);

    let _ = session.handle_key(Key::Home);
    let _ = session.handle_key(Key::Char('d'));
    assert_eq!(session.config.enabled_count(), 13);
    assert_eq!(
        session.config.global.download_connections,
        app::DOWNLOAD_CONNECTIONS
    );
    assert_eq!(session.config.global.format, Format::Text);

    // verify item parameter editing via inline modal on network.iperf3
    let iperf_pos = session
        .config
        .items
        .iter()
        .position(|item| item.id == "network.iperf3")
        .unwrap();
    for _ in 0..=iperf_pos {
        let _ = session.handle_key(Key::Down);
    }
    let _ = session.handle_key(Key::Tab);
    let _ = session.handle_key(Key::Down); // Countries field
    let _ = session.handle_key(Key::Enter);
    type_str(&mut session, "DE,SG");
    let _ = session.handle_key(Key::Enter);
    assert_eq!(
        session.config.items[iperf_pos].params,
        ItemParams::Iperf3 {
            countries: "DE,SG".to_owned(),
            servers: String::new(),
            seconds: 10,
            streams: 8,
            rate: None,
        }
    );

    let planned = session.config.build_plan(&all).unwrap();
    let ids: Vec<&str> = planned.iter().map(Planned::id).collect();
    assert_eq!(ids.len(), 13);
}

#[test]
fn full_lifecycle_running_and_results_viewer_render_and_navigate_cleanly() {
    let all = registry::all();
    let mut session = Session::new(&all);

    // Render Configure screen at standard and compact terminal dimensions without overflowing.
    for (cols, rows) in [(110_u16, 32_u16), (80, 24), (48, 12)] {
        let frame = tui::render::render(&mut session, cols, rows);
        let plain = frame.to_plain();
        for line in plain.lines() {
            assert!(
                line.width() <= usize::from(cols),
                "line exceeded {cols} cols: {line:?}"
            );
        }
    }

    // Transition to Running screen, emit live updates, scroll log, and test two-stage Ctrl-C.
    session.begin_run(&[
        ("cpu.sysbench", "CPU performance"),
        ("cpu.geekbench", "Geekbench 7"),
    ]);
    assert_eq!(session.screen, Screen::Running);
    session.set_elapsed(Duration::from_secs(42));
    session.set_host_summary("KVM, x86_64  |  IPv4: 198.51.*.*".to_owned());
    session.on_item_start("cpu.sysbench");
    session.on_item_step("cpu.sysbench", "single-thread test".to_owned());
    for idx in 0..30 {
        session.push_log(format!("[00:00:{idx:02}] cpu.sysbench step {idx}"));
    }
    session.on_item_finish("cpu.sysbench", "done in 20.1 s".to_owned(), false, false);
    session.on_item_start("cpu.geekbench");

    let running_frame = tui::render::render(&mut session, 100, 28).to_plain();
    assert!(running_frame.contains("[00:00:42]"), "{running_frame}");
    assert!(running_frame.contains("CPU performance"), "{running_frame}");
    assert!(running_frame.contains("Geekbench 7"), "{running_frame}");

    let _ = session.handle_key(Key::PageUp);
    assert!(!session.running.as_ref().unwrap().follow_log);
    let _ = session.handle_key(Key::End);
    assert!(session.running.as_ref().unwrap().follow_log);

    // First Ctrl-C requests cooperative cancel; second Ctrl-C exits with 130.
    assert_eq!(session.handle_key(Key::CtrlC), Command::CancelRun);
    assert_eq!(
        session.handle_key(Key::CtrlC),
        Command::Quit(app::EXIT_INTERRUPTED)
    );

    // Finish run and inspect Results viewer: sections, formats, wrapping, IP toggle, save, print.
    session.finish_run(sample_outcome(0), Some(72));
    assert_eq!(session.screen, Screen::Results);

    let text_frame = tui::render::render(&mut session, 104, 30).to_plain();
    assert!(text_frame.contains("Report Viewer"), "{text_frame}");
    assert!(text_frame.contains("Sections"), "{text_frame}");
    assert!(
        text_frame.contains("198.51.*.*"),
        "masked by default: {text_frame}"
    );

    // Toggle full IP ('i') -> unmasks 198.51.100.42!
    let _ = session.handle_key(Key::Char('i'));
    let unmasked_frame = tui::render::render(&mut session, 104, 30).to_plain();
    assert!(
        unmasked_frame.contains("198.51.100.42"),
        "full IP after 'i': {unmasked_frame}"
    );

    // Jump sections with ']' and '[' and pan horizontally with Right / Left after toggling wrap.
    let _ = session.handle_key(Key::Char('w'));
    let _ = session.handle_key(Key::Right);
    assert!(session.results.as_ref().unwrap().scroll_x > 0);
    let _ = session.handle_key(Key::Left);
    let _ = session.handle_key(Key::Char(']'));
    assert!(session.results.as_ref().unwrap().scroll_y > 0);
    let _ = session.handle_key(Key::Home);
    assert_eq!(session.results.as_ref().unwrap().scroll_y, 0);

    // Cycle format Text -> Markdown -> Json -> Text.
    let _ = session.handle_key(Key::Char('f'));
    assert_eq!(session.results.as_ref().unwrap().format, Format::Markdown);
    let md_frame = tui::render::render(&mut session, 104, 30).to_plain();
    assert!(md_frame.contains("# lowendaff-bench"), "{md_frame}");

    let _ = session.handle_key(Key::Char('f'));
    assert_eq!(session.results.as_ref().unwrap().format, Format::Json);

    // Save prompt ('s') emits SaveReport command with valid JSON report!
    let _ = session.handle_key(Key::Char('s'));
    assert!(session.editor.is_some());
    let cmd = session.handle_key(Key::Enter);
    match cmd {
        Command::SaveReport { path, contents } => {
            assert_eq!(path.to_str(), Some("report.json"));
            let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
            assert_eq!(parsed["summary"]["completed"], 1);
        }
        other => panic!("expected SaveReport command, got {other:?}"),
    }

    // Print & Quit ('p') returns rendered report; 'n' returns to Configure with settings intact.
    match session.handle_key(Key::Char('p')) {
        Command::PrintAndQuit { report, exit_code } => {
            assert_eq!(exit_code, 0);
            assert!(report.contains("\"cpu.geekbench\""));
        }
        other => panic!("expected PrintAndQuit, got {other:?}"),
    }

    assert_eq!(session.handle_key(Key::Char('n')), Command::None);
    assert_eq!(session.screen, Screen::Configure);
}
