//! End-to-end `ratatui::backend::TestBackend` and `crossterm::event` tests for `--tui`.

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
use bench::tui::Pane;
use bench::tui::Screen;
use bench::tui::Session;
use bench::tui::TuiConfig;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyEventState;
use crossterm::event::KeyModifiers;
use plugin_cpu::geekbench;
use plugin_cpu::geekbench::result::Scores;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(ch: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
}

fn release(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
    }
}

fn type_str(session: &mut Session, text: &str) {
    for ch in text.chars() {
        let _ = session.handle_key(press(KeyCode::Char(ch)));
    }
}

fn render_text(session: &mut Session, cols: u16, rows: u16) -> String {
    let backend = TestBackend::new(cols, rows);
    let mut terminal = Terminal::new(backend).expect("TestBackend initializes");
    terminal
        .draw(|frame| tui::ui::draw(frame, session))
        .expect("TestBackend draws");
    let buf = terminal.backend().buffer();
    let mut lines = Vec::with_capacity(usize::from(rows));
    for y in 0..rows {
        let mut row = String::with_capacity(usize::from(cols));
        for x in 0..cols {
            let symbol = buf[(x, y)].symbol();
            if !symbol.is_empty() {
                row.push_str(symbol);
            }
        }
        lines.push(row.trim_end().to_owned());
    }
    lines.join("\n")
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
fn release_events_and_stray_pastes_are_ignored_while_editor_supports_cursor_and_paste() {
    let all = registry::all();
    let mut session = Session::new(&all);

    // Key release events (e.g. Kitty protocol) must never trigger actions or toggles.
    assert_eq!(
        session.handle_key(release(KeyCode::Char('q'))),
        Command::None
    );
    assert_eq!(session.handle_key(release(KeyCode::Down)), Command::None);
    assert_eq!(session.nav_index, 0);

    // Bracketed paste outside an open editor must not trigger single-key shortcuts like 'a' or 'q'.
    session.handle_paste("aqdr");
    assert_eq!(session.config.enabled_count(), 13);

    // Open Global -> work_dir editor and exercise cursor movement, Delete, Backspace, and Paste.
    let _ = session.handle_key(press(KeyCode::End));
    let _ = session.handle_key(press(KeyCode::Enter));
    let _ = session.handle_key(press(KeyCode::Down));
    let _ = session.handle_key(press(KeyCode::Down)); // Work directory
    let _ = session.handle_key(press(KeyCode::Enter));
    assert!(session.editor.is_some());

    let _ = session.handle_key(ctrl('u'));
    session.handle_paste("/mnt/bench");
    let _ = session.handle_key(press(KeyCode::Left));
    let _ = session.handle_key(press(KeyCode::Left));
    let _ = session.handle_key(press(KeyCode::Left));
    let _ = session.handle_key(press(KeyCode::Left));
    let _ = session.handle_key(press(KeyCode::Left)); // cursor before "bench"
    type_str(&mut session, "nvme_");
    let _ = session.handle_key(press(KeyCode::Home));
    let _ = session.handle_key(press(KeyCode::Delete)); // remove leading '/'
    type_str(&mut session, "/");
    let _ = session.handle_key(press(KeyCode::End));
    let _ = session.handle_key(press(KeyCode::Enter));

    assert!(session.editor.is_none());
    assert_eq!(session.config.global.work_dir, "/mnt/nvme_bench");
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

    let bgp_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.bgp")
        .unwrap();
    assert_eq!(cfg.activate_item_field(bgp_idx, 1), None); // upload -> false

    assert_eq!(cfg.customized_count(), 5);
    let settings = cfg.plugin_settings();
    assert_eq!(
        settings["network.bgp"],
        serde_json::json!({
            "upload": false,
        })
    );
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

    let planned = cfg
        .build_plan(&all)
        .expect("customized settings must build");
    assert_eq!(planned.len(), 13);

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
    assert_eq!(session.handle_key(press(KeyCode::Down)), Command::None);
    assert_eq!(session.handle_key(press(KeyCode::Char(' '))), Command::None);
    assert!(!session.config.items[0].enabled);

    // Move to memory.sysbench (3) and press 'g' to toggle the entire memory group off.
    let _ = session.handle_key(press(KeyCode::Down));
    let _ = session.handle_key(press(KeyCode::Down));
    let _ = session.handle_key(press(KeyCode::Char('g')));
    for item in session.config.items.iter().filter(|i| i.group == "memory") {
        assert!(
            !item.enabled,
            "{} should be disabled by group toggle",
            item.id
        );
    }

    // Jump to Global options, focus right pane, cycle format, and validate connection count modal.
    let _ = session.handle_key(press(KeyCode::End));
    let _ = session.handle_key(press(KeyCode::Enter));
    assert_eq!(session.focus, Pane::Right);
    let _ = session.handle_key(press(KeyCode::Right));
    assert_eq!(session.config.global.format, Format::Markdown);

    for _ in 0..4 {
        let _ = session.handle_key(press(KeyCode::Down));
    }
    let _ = session.handle_key(press(KeyCode::Enter));
    assert!(session.editor.is_some());
    let _ = session.handle_key(ctrl('u'));
    type_str(&mut session, "999");
    let _ = session.handle_key(press(KeyCode::Enter));
    assert!(
        session.editor.is_some(),
        "invalid connection count keeps modal open with error"
    );
    let modal_frame = render_text(&mut session, 100, 28);
    assert!(modal_frame.contains("1 to 256"), "{modal_frame}");

    let _ = session.handle_key(ctrl('u'));
    type_str(&mut session, "16");
    let _ = session.handle_key(press(KeyCode::Enter));
    assert!(session.editor.is_none());
    assert_eq!(session.config.global.download_connections, 16);

    let _ = session.handle_key(press(KeyCode::Esc));
    assert_eq!(session.focus, Pane::Left);
    assert_eq!(
        session.handle_key(press(KeyCode::Char('r'))),
        Command::StartRun
    );

    let _ = session.handle_key(press(KeyCode::Home));
    let _ = session.handle_key(press(KeyCode::Char('d')));
    assert_eq!(session.config.enabled_count(), 13);
    assert_eq!(
        session.config.global.download_connections,
        app::DOWNLOAD_CONNECTIONS
    );
    assert_eq!(session.config.global.format, Format::Text);

    // Edit network.iperf3 Countries via modal.
    let iperf_pos = session
        .config
        .items
        .iter()
        .position(|item| item.id == "network.iperf3")
        .unwrap();
    for _ in 0..=iperf_pos {
        let _ = session.handle_key(press(KeyCode::Down));
    }
    let _ = session.handle_key(press(KeyCode::Tab));
    let _ = session.handle_key(press(KeyCode::Down));
    let _ = session.handle_key(press(KeyCode::Enter));
    type_str(&mut session, "DE,SG");
    let _ = session.handle_key(press(KeyCode::Enter));
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
fn ratatui_test_backend_renders_configure_running_and_results_across_sizes() {
    let all = registry::all();
    let mut session = Session::new(&all);

    for (cols, rows) in [(110_u16, 32_u16), (80, 24), (48, 12)] {
        let rendered = render_text(&mut session, cols, rows);
        assert!(!rendered.is_empty());
    }

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

    let running_frame = render_text(&mut session, 100, 28);
    assert!(running_frame.contains("[00:00:42]"), "{running_frame}");
    assert!(running_frame.contains("1 / 2 completed"), "{running_frame}");
    assert!(running_frame.contains("CPU performance"), "{running_frame}");
    assert!(running_frame.contains("Geekbench 7"), "{running_frame}");

    let _ = session.handle_key(press(KeyCode::PageUp));
    assert!(!session.running.as_ref().unwrap().follow_log);
    let _ = session.handle_key(press(KeyCode::End));
    assert!(session.running.as_ref().unwrap().follow_log);

    assert_eq!(session.handle_key(ctrl('c')), Command::CancelRun);
    assert_eq!(
        session.handle_key(ctrl('c')),
        Command::Quit(app::EXIT_INTERRUPTED)
    );

    session.finish_run(sample_outcome(0), Some(72));
    assert_eq!(session.screen, Screen::Results);

    let text_frame = render_text(&mut session, 104, 30);
    assert!(text_frame.contains("Report Viewer"), "{text_frame}");
    assert!(text_frame.contains("Sections"), "{text_frame}");
    assert!(
        text_frame.contains("198.51.*.*"),
        "masked by default: {text_frame}"
    );

    let _ = session.handle_key(press(KeyCode::Char('i')));
    let unmasked_frame = render_text(&mut session, 104, 30);
    assert!(
        unmasked_frame.contains("198.51.100.42"),
        "full IP after 'i': {unmasked_frame}"
    );

    let _ = session.handle_key(press(KeyCode::Char('w')));
    let _ = session.handle_key(press(KeyCode::Right));
    assert!(session.results.as_ref().unwrap().scroll_x > 0);
    let _ = session.handle_key(press(KeyCode::Left));
    let _ = session.handle_key(press(KeyCode::Char(']')));
    assert!(session.results.as_ref().unwrap().scroll_y > 0);
    let _ = session.handle_key(press(KeyCode::Home));
    assert_eq!(session.results.as_ref().unwrap().scroll_y, 0);

    let _ = session.handle_key(press(KeyCode::Char('f')));
    assert_eq!(session.results.as_ref().unwrap().format, Format::Markdown);
    let md_frame = render_text(&mut session, 104, 30);
    assert!(md_frame.contains("# lowendaff-bench"), "{md_frame}");

    let _ = session.handle_key(press(KeyCode::Char('f')));
    assert_eq!(session.results.as_ref().unwrap().format, Format::Json);

    let _ = session.handle_key(press(KeyCode::Char('s')));
    assert!(session.editor.is_some());
    match session.handle_key(press(KeyCode::Enter)) {
        Command::SaveReport { path, contents } => {
            assert_eq!(path.to_str(), Some("report.json"));
            let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
            assert_eq!(parsed["summary"]["completed"], 1);
        }
        other => panic!("expected SaveReport command, got {other:?}"),
    }

    match session.handle_key(press(KeyCode::Char('p'))) {
        Command::PrintAndQuit { report, exit_code } => {
            assert_eq!(exit_code, 0);
            assert!(report.contains("\"cpu.geekbench\""));
        }
        other => panic!("expected PrintAndQuit, got {other:?}"),
    }

    assert_eq!(session.handle_key(press(KeyCode::Char('n'))), Command::None);
    assert_eq!(session.screen, Screen::Configure);
}
