//! End-to-end `ratatui::backend::TestBackend` and `crossterm::event` tests for the interactive TUI.

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
use plugin_cpu::bench as cpu_bench;
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
        id: "cpu.bench",
        group: "cpu",
        title: Some("CPU benchmark"),
        started: Some(Duration::from_secs(0)),
        elapsed: Some(Duration::from_millis(1500)),
        status: Status::Completed,
        result: Some(Measured::cpu_bench(Report {
            output: Some(cpu_bench::Output {
                geekbench: Some(cpu_bench::SuiteOutcome::Done {
                    output: geekbench::Output {
                        scores: Some(Scores {
                            version: Some("7.1.0".to_owned()),
                            single: 2100,
                            multi: 9400,
                            url: None,
                        }),
                        ..geekbench::Output::default()
                    },
                }),
                ..cpu_bench::Output::default()
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
    assert_eq!(session.config.enabled_count(), 15);

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

    assert_eq!(cfg.items.len(), 15);
    assert_eq!(cfg.enabled_count(), 15);
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
    assert_eq!(cfg.global.socks5_proxy, "");
    assert_eq!(cfg.global.github_proxy, "");
    assert!(!cfg.global.verbose);

    let iperf_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.iperf3")
        .unwrap();
    cfg.apply_item_edit(iperf_idx, 1, " US , JP ").unwrap();
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

    let bgp_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.bgp")
        .unwrap();
    assert_eq!(cfg.activate_item_field(bgp_idx, 1), None); // upload -> false

    let cpu_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "cpu.bench")
        .unwrap();
    assert_eq!(cfg.activate_item_field(cpu_idx, 6), None); // ycruncher -> true
    assert_eq!(cfg.activate_item_field(cpu_idx, 8), None); // geekbench -> false

    let taier_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.taier")
        .unwrap();
    cfg.apply_item_edit(taier_idx, 2, " beijing:cu , guangdong/shenzhen:ct+cm ")
        .unwrap();
    cfg.apply_item_edit(taier_idx, 3, "6").unwrap();
    cfg.apply_item_edit(taier_idx, 4, "4").unwrap();
    assert!(
        cfg.apply_item_edit(taier_idx, 2, "beijing:unknownop")
            .is_err(),
        "unknown operator in selector must be rejected"
    );
    assert!(cfg.apply_item_edit(taier_idx, 3, "0").is_err());
    assert!(cfg.apply_item_edit(taier_idx, 4, "99").is_err());

    let cf_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.cloudflare")
        .unwrap();
    cfg.apply_item_edit(cf_idx, 1, "6").unwrap();
    cfg.apply_item_edit(cf_idx, 2, "4").unwrap();
    assert_eq!(cfg.activate_item_field(cf_idx, 3), None); // udp -> false
    assert!(cfg.apply_item_edit(cf_idx, 1, "0").is_err());
    assert!(cfg.apply_item_edit(cf_idx, 2, "99").is_err());

    let inet_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.inetspeed")
        .unwrap();
    cfg.apply_item_edit(inet_idx, 1, "8").unwrap();
    cfg.apply_item_edit(inet_idx, 2, "15").unwrap();
    cfg.apply_item_edit(inet_idx, 3, "500M").unwrap();
    cfg.apply_item_edit(inet_idx, 4, "10").unwrap();
    cfg.apply_item_edit(inet_idx, 5, "198.51.100.25").unwrap();
    assert!(cfg.apply_item_edit(inet_idx, 3, "10XB").is_err());
    assert!(cfg.apply_item_edit(inet_idx, 5, "not-an-ip").is_err());

    assert_eq!(cfg.customized_count(), 9);
    let settings = cfg.plugin_settings();
    assert_eq!(
        settings["network.cloudflare"],
        serde_json::json!({
            "seconds": 6,
            "concurrency": 4,
            "udp": false,
        })
    );
    assert_eq!(
        settings["network.inetspeed"],
        serde_json::json!({
            "threads": 8,
            "timeout": 15,
            "max": "500M",
            "latency_count": 10,
            "endpoint": "198.51.100.25",
        })
    );
    assert_eq!(
        settings["network.taier"],
        serde_json::json!({
            "selections": ["beijing:cu", "guangdong/shenzhen:ct+cm"],
            "seconds": 6,
            "streams": 4,
        })
    );
    assert_eq!(
        settings["cpu.bench"],
        serde_json::json!({
            "suites": ["sysbench", "7zip", "unixbench", "hpl", "passmark", "ycruncher"],
        })
    );
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
            "seconds": 15,
            "streams": 4,
            "rate": 500,
        })
    );
    assert_eq!(
        settings["network.trace"],
        serde_json::json!({
            "groups": ["china"],
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
        })
    );

    let planned = cfg
        .build_plan(&all)
        .expect("customized settings must build");
    assert_eq!(planned.len(), 15);
    cfg.items[taier_idx].reset();

    // Unchecking every sub-suite in cpu.bench while cpu.bench is enabled is rejected.
    if let ItemParams::CpuBench { suites } = &mut cfg.items[cpu_idx].params {
        *suites = [false; 8];
    }
    assert!(cfg.build_plan(&all).is_err());

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
    let memory_items = session.config.items.iter().filter(|i| i.group == "memory");
    for item in memory_items {
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

    // Global field 5: SOCKS5 proxy rejects http://, accepts socks5h://, and redacts password.
    let _ = session.handle_key(press(KeyCode::Down));
    let _ = session.handle_key(press(KeyCode::Enter));
    assert!(session.editor.is_some());
    type_str(&mut session, "http://127.0.0.1:1080");
    let _ = session.handle_key(press(KeyCode::Enter));
    assert!(
        session.editor.is_some(),
        "non-SOCKS5 scheme keeps modal open with error"
    );
    let _ = session.handle_key(ctrl('u'));
    type_str(&mut session, "socks5h://alice:s3cret@127.0.0.1:1080");
    let _ = session.handle_key(press(KeyCode::Enter));
    assert!(session.editor.is_none());
    assert_eq!(
        session.config.global.socks5_proxy,
        "socks5h://alice:s3cret@127.0.0.1:1080"
    );
    let global_frame = render_text(&mut session, 140, 28);
    assert!(
        global_frame.contains("socks5h://alice:***@127.0.0.1:1080")
            && !global_frame.contains("s3cret"),
        "rendered TUI row must mask proxy password: {global_frame}"
    );

    // Global field 6: GitHub proxy URL rejects bare hostname and accepts https://ghfast.top/.
    let _ = session.handle_key(press(KeyCode::Down));
    let _ = session.handle_key(press(KeyCode::Enter));
    type_str(&mut session, "ghfast.top");
    let _ = session.handle_key(press(KeyCode::Enter));
    assert!(session.editor.is_some());
    let _ = session.handle_key(ctrl('u'));
    type_str(&mut session, "https://ghfast.top/");
    let _ = session.handle_key(press(KeyCode::Enter));
    assert!(session.editor.is_none());
    assert_eq!(session.config.global.github_proxy, "https://ghfast.top/");

    // Global field 7: Verbose debug log toggle.
    let _ = session.handle_key(press(KeyCode::Down));
    let _ = session.handle_key(press(KeyCode::Char(' ')));
    assert!(session.config.global.verbose);

    let _ = session.handle_key(press(KeyCode::Esc));
    assert_eq!(session.focus, Pane::Left);
    assert_eq!(
        session.handle_key(press(KeyCode::Char('r'))),
        Command::StartRun
    );

    let _ = session.handle_key(press(KeyCode::Home));
    let _ = session.handle_key(press(KeyCode::Char('d')));
    assert_eq!(session.config.enabled_count(), 15);
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
    assert_eq!(ids.len(), 15);
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
        ("memory.sysbench", "Memory sysbench"),
        ("cpu.bench", "CPU benchmark"),
    ]);
    assert_eq!(session.screen, Screen::Running);
    session.set_elapsed(Duration::from_secs(42));
    session.set_host_summary("KVM, x86_64  |  IPv4: 198.51.*.*".to_owned());
    session.on_item_start("memory.sysbench");
    session.on_item_step("memory.sysbench", "single-thread read".to_owned());
    for idx in 0..30 {
        session.push_log(format!("[00:00:{idx:02}] memory.sysbench step {idx}"));
    }
    session.on_item_finish("memory.sysbench", "done in 20.1 s".to_owned(), false, false);
    session.on_item_start("cpu.bench");

    let running_frame = render_text(&mut session, 100, 28);
    assert!(running_frame.contains("[00:00:42]"), "{running_frame}");
    assert!(running_frame.contains("1 / 2 completed"), "{running_frame}");
    assert!(running_frame.contains("Memory sysbench"), "{running_frame}");
    assert!(running_frame.contains("CPU benchmark"), "{running_frame}");

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
            let path_str = path.to_str().unwrap();
            assert!(
                path_str.starts_with("/tmp/lowendaff-bench-") && path_str.ends_with(".json"),
                "default save path must be under /tmp: {path_str}"
            );
            let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
            assert_eq!(parsed["summary"]["completed"], 1);
        }
        other => panic!("expected SaveReport command, got {other:?}"),
    }

    match session.handle_key(press(KeyCode::Char('p'))) {
        Command::PrintAndQuit { report, exit_code } => {
            assert_eq!(exit_code, 0);
            assert!(report.contains("\"cpu.bench\""));
        }
        other => panic!("expected PrintAndQuit, got {other:?}"),
    }

    assert_eq!(session.handle_key(press(KeyCode::Char('n'))), Command::None);
    assert_eq!(session.screen, Screen::Configure);
}

#[test]
fn disabling_items_clears_sub_selections_and_editing_sub_options_re_enables_bidirectionally() {
    let all = registry::all();
    let mut cfg = TuiConfig::from_registry(&all);

    let trace_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.trace")
        .unwrap();
    // 1. network.trace is enabled -> select china -> switch to disabled -> selections clear!
    cfg.activate_item_field(trace_idx, 2); // china -> true
    assert!(cfg.items[trace_idx].is_customized());
    cfg.activate_item_field(trace_idx, 0); // Enabled -> false
    assert!(!cfg.items[trace_idx].enabled);
    assert!(!cfg.items[trace_idx].is_customized());
    assert!(!cfg.plugin_settings().contains_key("network.trace"));
    assert_eq!(
        cfg.items[trace_idx].params,
        ItemParams::Trace {
            groups: [false; 4],
            targets: String::new(),
            geo: false,
        }
    );
    assert!(
        cfg.fields_for_item(trace_idx)[1..]
            .iter()
            .all(|row| row.dimmed && !row.customized),
        "sub-rows of a disabled item must be dimmed and unstarred"
    );

    // 2. Toggling a sub-group while disabled automatically re-enables network.trace with geo=true.
    cfg.activate_item_field(trace_idx, 2); // china -> true
    assert!(cfg.items[trace_idx].enabled);
    assert_eq!(
        cfg.items[trace_idx].params,
        ItemParams::Trace {
            groups: [false, true, false, false],
            targets: String::new(),
            geo: true,
        }
    );

    // 3. `all` checkbox syncs bidirectionally with individual groups, and overrides clear groups.
    cfg.activate_item_field(trace_idx, 1); // services -> true
    cfg.activate_item_field(trace_idx, 3); // international -> true -> all auto-checks!
    assert_eq!(
        cfg.items[trace_idx].params,
        ItemParams::Trace {
            groups: [true; 4],
            targets: String::new(),
            geo: true,
        }
    );
    assert_eq!(
        cfg.plugin_settings()["network.trace"],
        serde_json::json!({ "groups": ["all"] })
    );
    cfg.activate_item_field(trace_idx, 2); // uncheck china -> all auto-unchecks!
    assert_eq!(
        cfg.items[trace_idx].params,
        ItemParams::Trace {
            groups: [true, false, true, false],
            targets: String::new(),
            geo: true,
        }
    );
    cfg.apply_item_edit(trace_idx, 5, "youtube").unwrap();
    assert_eq!(
        cfg.items[trace_idx].params,
        ItemParams::Trace {
            groups: [false; 4],
            targets: "youtube".to_owned(),
            geo: true,
        },
        "entering target override clears group checkboxes"
    );
    cfg.activate_item_field(trace_idx, 4); // check `all` -> clears target override!
    assert_eq!(
        cfg.items[trace_idx].params,
        ItemParams::Trace {
            groups: [true; 4],
            targets: String::new(),
            geo: true,
        }
    );

    // 4. cpu.bench: unchecking all 8 suites auto-disables; master re-enable restores 6 defaults;
    //    checking one suite while disabled enables cpu.bench with only that suite.
    let cpu_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "cpu.bench")
        .unwrap();
    for field in [1_usize, 2, 3, 4, 5, 8] {
        cfg.activate_item_field(cpu_idx, field);
    }
    assert!(
        !cfg.items[cpu_idx].enabled,
        "unchecking all CPU suites must auto-disable cpu.bench"
    );
    cfg.activate_item_field(cpu_idx, 0); // master re-enable
    assert!(cfg.items[cpu_idx].enabled);
    assert_eq!(
        cfg.items[cpu_idx].params,
        ItemParams::default_for("cpu.bench")
    );
    cfg.activate_item_field(cpu_idx, 0); // master disable -> all [false; 8]
    assert_eq!(
        cfg.items[cpu_idx].params,
        ItemParams::CpuBench { suites: [false; 8] }
    );
    cfg.activate_item_field(cpu_idx, 2); // check 7zip while disabled
    assert!(cfg.items[cpu_idx].enabled);
    assert_eq!(
        cfg.plugin_settings()["cpu.bench"],
        serde_json::json!({ "suites": ["7zip"] })
    );

    // 5. Mutual override clearing on network.iperf3 and ip.media.
    let iperf_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.iperf3")
        .unwrap();
    cfg.apply_item_edit(iperf_idx, 1, "JP").unwrap();
    cfg.apply_item_edit(iperf_idx, 2, "tokyo-1").unwrap();
    assert_eq!(
        cfg.plugin_settings()["network.iperf3"],
        serde_json::json!({ "servers": ["tokyo-1"] })
    );
    cfg.toggle_group("network");
    assert!(!cfg.items[iperf_idx].enabled);
    assert!(!cfg.plugin_settings().contains_key("network.iperf3"));
    cfg.step_item_field(iperf_idx, 3, 5); // stepping duration re-enables network.iperf3
    assert!(cfg.items[iperf_idx].enabled);
    assert_eq!(
        cfg.plugin_settings()["network.iperf3"],
        serde_json::json!({ "seconds": 15 })
    );

    // 6. network.taier: editing target selectors or stepping duration while disabled re-enables
    //    network.taier; disabling clears target selectors back to defaults.
    let taier_idx = cfg
        .items
        .iter()
        .position(|item| item.id == "network.taier")
        .unwrap();
    assert!(!cfg.items[taier_idx].enabled);
    cfg.apply_item_edit(taier_idx, 2, "beijing:cu").unwrap();
    assert!(cfg.items[taier_idx].enabled);
    assert_eq!(
        cfg.plugin_settings()["network.taier"],
        serde_json::json!({ "selections": ["beijing:cu"] })
    );
    cfg.activate_item_field(taier_idx, 0); // master disable -> clears selections!
    assert!(!cfg.items[taier_idx].enabled);
    assert!(!cfg.plugin_settings().contains_key("network.taier"));
    cfg.step_item_field(taier_idx, 3, 2); // step duration while disabled -> re-enables!
    assert!(cfg.items[taier_idx].enabled);
    assert_eq!(
        cfg.plugin_settings()["network.taier"],
        serde_json::json!({ "seconds": 6 })
    );
}

#[test]
fn taier_hierarchical_picker_flattens_municipalities_drills_provinces_searches_and_roundtrips() {
    use plugin_network::taier::nodes;

    let raw = br#"{
        "version": 1,
        "locations": [
            {
                "id": "beijing",
                "province": "Beijing",
                "city": "Beijing",
                "default": true,
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.1", "port": 65499},
                    {"operator": "unicom", "ip": "198.51.100.2", "port": 65499},
                    {"operator": "mobile", "ip": "198.51.100.3", "port": 65499},
                    {"operator": "cernet", "ip": "198.51.100.4", "port": 65499}
                ]
            },
            {
                "id": "shanghai",
                "province": "Shanghai",
                "city": "Shanghai",
                "default": true,
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.11", "port": 65499},
                    {"operator": "unicom", "ip": "198.51.100.12", "port": 65499},
                    {"operator": "mobile", "ip": "198.51.100.13", "port": 65499}
                ]
            },
            {
                "id": "guangdong-guangzhou",
                "province": "Guangdong",
                "city": "Guangzhou",
                "default": true,
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.21", "port": 65499},
                    {"operator": "unicom", "ip": "198.51.100.22", "port": 65499},
                    {"operator": "mobile", "ip": "198.51.100.23", "port": 9443},
                    {"operator": "mobile", "ip": "198.51.100.24", "port": 9443}
                ]
            },
            {
                "id": "guangdong-shenzhen",
                "province": "Guangdong",
                "city": "Shenzhen",
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.31", "port": 65499},
                    {"operator": "mobile", "ip": "198.51.100.32", "port": 9443}
                ]
            },
            {
                "id": "jilin",
                "province": "Jilin",
                "city": "Jilin",
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.41", "port": 65499}
                ]
            },
            {
                "id": "jilin-changchun",
                "province": "Jilin",
                "city": "Changchun",
                "nodes": [
                    {"operator": "unicom", "ip": "198.51.100.42", "port": 65499}
                ]
            }
        ]
    }"#;
    let locations = nodes::parse(raw).expect("valid fixture");
    let all = registry::all();
    let mut session = Session::new(&all);

    // Navigate to network.taier -> Right pane -> Row 1 ("Select nodes (tree)").
    let taier_pos = session
        .config
        .items
        .iter()
        .position(|item| item.id == "network.taier")
        .unwrap();
    for _ in 0..=taier_pos {
        let _ = session.handle_key(press(KeyCode::Down));
    }
    let _ = session.handle_key(press(KeyCode::Enter)); // Focus right pane
    let _ = session.handle_key(press(KeyCode::Down)); // Row 1: Select nodes (tree)

    // 1. First open triggers background LoadTaierNodes and renders Loading -> Error -> Retry -> Ready.
    assert_eq!(
        session.handle_key(press(KeyCode::Enter)),
        Command::LoadTaierNodes
    );
    let loading_frame = render_text(&mut session, 100, 28);
    assert!(
        loading_frame.contains("Fetching CAICT GlobalSpeed node list"),
        "{loading_frame}"
    );
    session.on_taier_catalog_loaded(Err("network unreachable".to_owned()));
    let error_frame = render_text(&mut session, 100, 28);
    assert!(error_frame.contains("network unreachable"), "{error_frame}");
    assert_eq!(
        session.handle_key(press(KeyCode::Char('r'))),
        Command::LoadTaierNodes
    );
    session.on_taier_catalog_loaded(Ok(locations.clone()));
    let ready_frame = render_text(&mut session, 100, 28);
    assert!(
        ready_frame.contains("Beijing") && ready_frame.contains("(municipality)"),
        "{ready_frame}"
    );
    assert!(ready_frame.contains("9 slots (~1m 12s)"), "{ready_frame}");

    // 2. Clear all ('c'), verify 's' refuses 0 slots with inline error, then pick custom nodes.
    let _ = session.handle_key(press(KeyCode::Char('c')));
    let _ = session.handle_key(press(KeyCode::Char('s')));
    assert!(
        session.taier_picker.is_some(),
        "applying with 0 slots must keep picker open"
    );
    let empty_err_frame = render_text(&mut session, 100, 28);
    assert!(
        empty_err_frame.contains("Select at least one carrier node"),
        "{empty_err_frame}"
    );

    // 3. Municipality (Beijing, row 0): Enter drills DIRECTLY into carriers (2 levels, no city row!).
    let _ = session.handle_key(press(KeyCode::Enter));
    let bj_frame = render_text(&mut session, 100, 28);
    assert!(bj_frame.contains("All Regions > Beijing"), "{bj_frame}");
    assert!(bj_frame.contains("CERNET Education (edu)"), "{bj_frame}");
    let _ = session.handle_key(press(KeyCode::Down)); // China Unicom (cu)
    let _ = session.handle_key(press(KeyCode::Char(' '))); // toggle cu ON
    let _ = session.handle_key(press(KeyCode::Left)); // directly back to All Regions!
    let back_frame = render_text(&mut session, 100, 28);
    assert!(back_frame.contains("All Regions "), "{back_frame}");

    // 4. Search filter ('/shen') -> Guangdong -> Enter (Cities: Shenzhen) -> Enter (Operators) -> Mobile.
    let _ = session.handle_key(press(KeyCode::Char('/')));
    type_str(&mut session, "shen");
    let _ = session.handle_key(press(KeyCode::Enter)); // exit filter input
    let _ = session.handle_key(press(KeyCode::Enter)); // drill into Guangdong -> Cities (filtered to Shenzhen)
    let gd_cities_frame = render_text(&mut session, 100, 28);
    assert!(
        gd_cities_frame.contains("All Regions > Guangdong") && gd_cities_frame.contains("Shenzhen"),
        "{gd_cities_frame}"
    );
    let _ = session.handle_key(press(KeyCode::Enter)); // drill into Shenzhen -> Operators (3rd level!)
    let sz_ops_frame = render_text(&mut session, 100, 28);
    assert!(
        sz_ops_frame.contains("All Regions > Guangdong > Shenzhen"),
        "{sz_ops_frame}"
    );
    let _ = session.handle_key(press(KeyCode::Down)); // China Mobile (cm)
    let _ = session.handle_key(press(KeyCode::Enter)); // toggle cm ON at leaf
    let _ = session.handle_key(press(KeyCode::Esc)); // up to Guangdong cities
    let _ = session.handle_key(press(KeyCode::Esc)); // up to All Regions
    let _ = session.handle_key(press(KeyCode::Esc)); // clear "shen" filter

    // 5. Homonymous Jilin province (row 3: Beijing, Shanghai, Guangdong, Jilin) -> Changchun + Jilin city!
    let _ = session.handle_key(press(KeyCode::End)); // jump to Jilin
    let _ = session.handle_key(press(KeyCode::Enter)); // Jilin has 2 cities -> must drill into Cities (3-level)!
    let jl_frame = render_text(&mut session, 100, 28);
    assert!(
        jl_frame.contains("All Regions > Jilin") && jl_frame.contains("Changchun"),
        "{jl_frame}"
    );
    let _ = session.handle_key(press(KeyCode::Down)); // move to Jilin city
    let _ = session.handle_key(press(KeyCode::Char(' '))); // toggle Jilin city (ct) ON

    // 6. Save directly from sub-level with 's' and verify exact round-trip selector string!
    let _ = session.handle_key(press(KeyCode::Char('s')));
    assert!(session.taier_picker.is_none());
    assert_eq!(
        session.config.items[taier_pos].params,
        ItemParams::Taier {
            selections: "beijing:cu,guangdong/shenzhen:cm,jilin/jilin:ct".to_owned(),
            seconds: 4,
            streams: 8,
        }
    );

    // 7. Re-open from cache (no LoadTaierNodes needed!), press 'd' for defaults, Esc applies "".
    assert_eq!(session.handle_key(press(KeyCode::Enter)), Command::None);
    assert!(session.taier_picker.is_some());
    let _ = session.handle_key(press(KeyCode::Char('d')));
    let _ = session.handle_key(press(KeyCode::Esc));
    assert!(session.taier_picker.is_none());
    assert!(session.config.items[taier_pos].params.is_default());
}
