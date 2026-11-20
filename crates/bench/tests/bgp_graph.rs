//! Stress and regression tests for `bench::report::bgp_graph`: dense multi-upstream × multi-Tier-1
//! meshes (two-port wire isolation without phantom row welds), heavy `AS_PATH` prepending (`×9`
//! clean-hop column compression and non-overlapping badges), CJK wide-glyph alignment, compact-box
//! scaling on 80-column terminals, and `(↑)` shared-subtree markers in narrow-TTY tree fallback.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use bench::report;
use plugin_network::bgp::BgpPath;
use plugin_network::bgp::FamilyBgp;
use plugin_network::bgp::OriginSummary;
use plugin_network::bgp::RpkiStatus;
use plugin_network::bgp::UpstreamShare;
use plugin_network::bgp::parse;
use unicode_width::UnicodeWidthStr;

fn path(raw: &[u32], peers: u32, is_best: bool) -> BgpPath {
    BgpPath {
        asns: parse::clean_path(raw),
        asns_raw: raw.to_vec(),
        peers,
        is_best,
    }
}

#[test]
fn crisscrossing_mesh_has_unique_cells_and_no_phantom_row_connections() {
    let mut names = BTreeMap::new();
    names.insert(64500, "Origin VPS".to_owned());
    names.insert(50001, "Up-Alpha".to_owned());
    names.insert(50002, "Up-Beta".to_owned());
    names.insert(50003, "Up-Gamma".to_owned());
    names.insert(50004, "Up-Delta".to_owned());

    let fam = FamilyBgp {
        address: "198.51.100.1".parse().expect("ip"),
        announced: true,
        prefix: Some("198.51.100.0/24".to_owned()),
        prefix_base: Some("198.51.100.0".parse().expect("base")),
        prefix_len: Some(24),
        origin_asn: Some(64500),
        origins: vec![OriginSummary {
            asn: 64500,
            peers: 100,
        }],
        rpki: RpkiStatus::Valid,
        total_peers: 100,
        distinct_paths: 8,
        upstreams: vec![
            UpstreamShare {
                asn: 50001,
                peers: 35,
            },
            UpstreamShare {
                asn: 50002,
                peers: 30,
            },
            UpstreamShare {
                asn: 50003,
                peers: 20,
            },
            UpstreamShare {
                asn: 50004,
                peers: 15,
            },
        ],
        paths: vec![
            path(&[3356, 50001, 64500], 20, true),
            path(&[2914, 50001, 64500], 15, false),
            path(&[174, 50002, 64500], 18, false),
            path(&[2914, 50002, 64500], 12, false),
            path(&[1299, 50003, 64500], 12, false),
            path(&[3356, 50003, 64500], 8, false),
            // AS50004 connects ONLY to AS174 and AS1299, never to AS2914 or AS3356.
            path(&[174, 50004, 64500], 10, false),
            path(&[1299, 50004, 64500], 5, false),
        ],
        asn_names: names,
    };

    let g = report::bgp_graph::layout(&fam, Some(report::bgp_graph::COMPACT_MAX_PATHS))
        .expect("layout");
    let mut occupied = BTreeSet::new();
    for b in &g.boxes {
        assert!(
            occupied.insert((b.col, b.row)),
            "duplicate box at col={} row={}",
            b.col,
            b.row
        );
    }

    let lines = report::bgp_graph::render_terminal(&g, "    ", Some(80));
    let joined = lines.join("\n");
    assert!(
        joined.contains("╭──────────────╮"),
        "fits standard 2D boxes in 80 columns"
    );
    for line in &lines {
        assert!(
            line.width() <= 80,
            "line exceeded 80 columns ({}): {line}",
            line.width()
        );
    }

    // Locate the box row where AS50004 sits in col 1 and verify that if an unconnected Tier-1 sits
    // on the same row in col 2, no straight horizontal arrow `▸` enters line 1 (`y0 + 1`) of that
    // col-2 box.
    let delta = g
        .boxes
        .iter()
        .find(|b| b.asn == 50004 && b.col == 1)
        .expect("AS50004 box");
    if let Some(same_row_t1) = g.boxes.iter().find(|b| b.col == 2 && b.row == delta.row)
        && same_row_t1.asn != 174
        && same_row_t1.asn != 1299
    {
        let y_out_line = &lines[2 + delta.row * 4];
        let needle = format!("▸│ AS{}", same_row_t1.asn);
        assert!(
            !y_out_line.contains(&needle),
            "phantom straight connection into unconnected AS{}: {y_out_line}",
            same_row_t1.asn
        );
    }
}

#[test]
fn heavy_prepending_uses_clean_hop_columns_and_keeps_all_badges_intact() {
    let mut names = BTreeMap::new();
    names.insert(64510, "Example Host".to_owned());

    let fam = FamilyBgp {
        address: "192.0.2.1".parse().expect("ip"),
        announced: true,
        prefix: Some("192.0.2.0/24".to_owned()),
        prefix_base: Some("192.0.2.0".parse().expect("base")),
        prefix_len: Some(24),
        origin_asn: Some(64510),
        origins: vec![OriginSummary {
            asn: 64510,
            peers: 100,
        }],
        rpki: RpkiStatus::Valid,
        total_peers: 100,
        distinct_paths: 6,
        upstreams: vec![UpstreamShare {
            asn: 6461,
            peers: 40,
        }],
        paths: vec![
            path(&[6461, 64510], 40, true),
            path(&[3491, 3356, 64510, 64510, 64510], 20, false),
            path(&[1299, 64510, 64510, 64510, 64510], 15, false),
            path(&[6762, 64510, 64510, 64510, 64510, 64510], 10, false),
            path(
                &[
                    3257, 64510, 64510, 64510, 64510, 64510, 64510, 64510, 64510, 64510,
                ],
                10,
                false,
            ),
            path(&[6830, 64520, 64520, 64520, 64510], 5, false),
        ],
        asn_names: names,
    };

    let g = report::bgp_graph::layout(&fam, Some(report::bgp_graph::COMPACT_MAX_PATHS))
        .expect("layout");
    assert_eq!(
        g.max_col, 2,
        "×9 prepending must not inflate clean-hop column count beyond 2"
    );

    let lines = report::bgp_graph::render_terminal(&g, "      ", Some(80));
    let joined = lines.join("\n");
    assert!(
        joined.contains("╭──────────────╮"),
        "renders full 2D boxes even on 80-col terminal despite ×9 prepending"
    );
    for badge in ["×3", "×4", "×5", "×9"] {
        assert!(
            joined.contains(badge),
            "missing prepend badge {badge}:\n{joined}"
        );
    }
    for line in &lines {
        assert!(line.width() <= 80, "line exceeded 80 cols: {line}");
    }
}

#[test]
fn four_column_wide_glyph_dag_scales_to_compact_boxes_at_80_cols_and_marks_shared_tree_nodes() {
    // Use `\u{...}` escapes so source stays ASCII-clean while exercising 2-column East Asian width
    // alignment inside 16-column and 13-column rounded boxes.
    let origin_label = "\u{8d77}\u{6e90}\u{8282}\u{70b9}";
    let transit_label = "\u{6c47}\u{805a}\u{4e2d}\u{8f6c}";
    let mut names = BTreeMap::new();
    names.insert(64500, origin_label.to_owned());
    names.insert(4809, "\u{4e2d}\u{56fd}\u{7535}\u{4fe1} CN2".to_owned());
    names.insert(9929, "\u{4e2d}\u{56fd}\u{8054}\u{901a} 9929".to_owned());
    names.insert(50010, transit_label.to_owned());

    let fam = FamilyBgp {
        address: "203.0.113.8".parse().expect("ip"),
        announced: true,
        prefix: Some("203.0.113.0/24".to_owned()),
        prefix_base: Some("203.0.113.0".parse().expect("base")),
        prefix_len: Some(24),
        origin_asn: Some(64500),
        origins: vec![OriginSummary {
            asn: 64500,
            peers: 50,
        }],
        rpki: RpkiStatus::Valid,
        total_peers: 50,
        distinct_paths: 4,
        upstreams: vec![
            UpstreamShare {
                asn: 4809,
                peers: 30,
            },
            UpstreamShare {
                asn: 9929,
                peers: 20,
            },
        ],
        paths: vec![
            // Both AS4809 and AS9929 converge into transit AS50010 at col 2, which fans out to
            // AS3356 and AS1299 at col 3 (a 4-column diamond DAG).
            path(&[3356, 50010, 4809, 64500], 20, true),
            path(&[1299, 50010, 4809, 64500], 10, false),
            path(&[3356, 50010, 9929, 64500], 12, false),
            path(&[1299, 50010, 9929, 64500], 8, false),
        ],
        asn_names: names,
    };

    let g = report::bgp_graph::layout(&fam, Some(report::bgp_graph::COMPACT_MAX_PATHS))
        .expect("layout");
    assert_eq!(g.max_col, 3);

    // At 100 columns: standard 16-column boxes (`╭──────────────╮`).
    let wide = report::bgp_graph::render_terminal(&g, "    ", Some(100));
    assert!(wide.iter().any(|l| l.contains("╭──────────────╮")));
    assert!(wide.iter().all(|l| l.width() <= 100));

    // At 80 columns: auto-scales to 13-column compact boxes (`╭───────────╮`) with CJK intact.
    let compact = report::bgp_graph::render_terminal(&g, "    ", Some(80));
    assert!(
        compact.iter().any(|l| l.contains("╭───────────╮")),
        "auto-scales to compact 2D boxes at 80 cols"
    );
    assert!(compact.iter().all(|l| l.width() <= 80));

    // At 34 columns (too narrow for two boxes side-by-side): falls back to Unicode tree and marks
    // the second visit to shared transit AS50010 with `(↑)`.
    let narrow = report::bgp_graph::render_terminal(&g, "    ", Some(34)).join("\n");
    assert!(narrow.contains(&format!("AS64500 {origin_label} (origin)")));
    assert!(
        narrow.contains(&format!("AS50010 {transit_label} (↑)")),
        "marks shared DAG node in tree fallback:\n{narrow}"
    );
}

#[test]
fn deep_seven_layer_hierarchy_wraps_into_continuation_bands_on_80_col_terminal() {
    let mut names = BTreeMap::new();
    names.insert(400100, "Leaf Origin".to_owned());
    names.insert(400200, "VM Host".to_owned());

    let fam = FamilyBgp {
        address: "2001:db8:dead::1".parse().expect("ipv6"),
        announced: true,
        prefix: Some("2001:db8:dead::/48".to_owned()),
        prefix_base: Some("2001:db8:dead::".parse().expect("base")),
        prefix_len: Some(48),
        origin_asn: Some(400100),
        origins: vec![OriginSummary {
            asn: 400100,
            peers: 120,
        }],
        rpki: RpkiStatus::Valid,
        total_peers: 120,
        distinct_paths: 4,
        upstreams: vec![UpstreamShare {
            asn: 400200,
            peers: 120,
        }],
        paths: vec![
            path(&[2914, 58453, 9808, 56048, 61112, 400200, 400100], 60, true),
            path(
                &[3356, 58453, 9808, 56048, 61112, 400200, 400100],
                25,
                false,
            ),
            path(&[1299, 4837, 4808, 61112, 400200, 400100], 20, false),
            path(&[174, 51847, 51847, 400200, 400100], 15, false),
        ],
        asn_names: names,
    };

    let g = report::bgp_graph::layout(&fam, Some(report::bgp_graph::COMPACT_MAX_PATHS))
        .expect("layout");
    assert_eq!(g.max_col, 6, "preserves all 7 clean hop layers (0..=6)");

    let lines = report::bgp_graph::render_terminal(&g, "    ", Some(80));
    let joined = lines.join("\n");
    assert!(
        joined.contains("╭──────────────╮"),
        "keeps 2D rounded boxes across wrapped bands instead of collapsing to a flat list"
    );
    assert!(
        joined.contains("2 (cont.)") && joined.contains("4 (cont.)"),
        "emits bridge-column continuation headers:\n{joined}"
    );
    assert!(
        joined.contains("│══▸"),
        "marks continuing boundary boxes with right-hand continuation stubs:\n{joined}"
    );
    for line in &lines {
        assert!(
            line.width() <= 80,
            "wrapped band line exceeded 80 columns ({}): {line}",
            line.width()
        );
    }
}
