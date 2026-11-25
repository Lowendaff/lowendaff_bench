//! Layout and vector-renderer regression tests for `bench::report::bgp_graph`: dense
//! multi-upstream × multi-Tier-1 meshes (unique `(col, row)` placement and main-path ordering),
//! heavy `AS_PATH` prepending (`×9` capped-span hop columns, unobstructed waypoint routing, and
//! pill badges), vertical fork centering, CJK carrier-name escaping in SVG and Mermaid, and
//! 7-layer deep hierarchy layout preservation.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use bench::report;
use plugin_network::bgp::BgpPath;
use plugin_network::bgp::FamilyBgp;
use plugin_network::bgp::OriginSummary;
use plugin_network::bgp::RpkiStatus;
use plugin_network::bgp::UpstreamShare;
use plugin_network::bgp::parse;

fn path(raw: &[u32], peers: u32, is_best: bool) -> BgpPath {
    BgpPath {
        asns: parse::clean_path(raw),
        asns_raw: raw.to_vec(),
        peers,
        is_best,
    }
}

#[test]
fn crisscrossing_mesh_has_unique_cells_and_clean_svg_and_mermaid_edges() {
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
            path(&[174, 50004, 64500], 10, false),
            path(&[1299, 50004, 64500], 5, false),
        ],
        asn_names: names,
        diagram_url: None,
    };

    let g = report::bgp_graph::layout(&fam, None).expect("layout");
    let mut occupied = BTreeSet::new();
    for b in &g.boxes {
        assert!(
            occupied.insert((b.col, b.row)),
            "duplicate box at col={} row={}",
            b.col,
            b.row
        );
    }

    let svg = report::bgp_graph::render_svg(&g);
    assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg"));
    assert!(svg.contains("AS64500</text>"));
    assert!(svg.contains("AS3356 &#x2605;</text>"));
    assert!(!svg.contains("198.51.100."));

    let mermaid = report::bgp_graph::render_mermaid(&g);
    assert!(mermaid.starts_with("flowchart LR\n"));
    assert!(mermaid.contains("n_64500_0 ==> n_50001_1"));
    assert!(mermaid.contains("n_50001_1 ==> n_3356_2"));
}

#[test]
fn heavy_prepending_caps_column_span_routes_around_boxes_and_renders_all_badges() {
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
        upstreams: vec![
            UpstreamShare {
                asn: 6461,
                peers: 40,
            },
            UpstreamShare {
                asn: 9002,
                peers: 12,
            },
        ],
        paths: vec![
            path(&[6461, 64510], 40, true),
            path(&[3491, 3356, 64510, 64510, 64510], 20, false),
            path(&[1299, 64510, 64510, 64510, 64510], 15, false),
            path(&[65001, 9002, 64510], 12, false),
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
        diagram_url: None,
    };

    let g = report::bgp_graph::layout(&fam, None).expect("layout");
    assert_eq!(
        g.max_col, 4,
        "×9 prepending advances by MAX_PREPEND_SPAN (4) while preserving outer Tier-1 hops"
    );
    assert!(
        g.boxes.iter().any(|b| b.asn == 9002 && b.col == 1),
        "uncovered non-Tier-1 direct upstream (>=5% share) must be supplemented in the graph"
    );
    assert!(
        g.boxes.iter().any(|b| b.asn == 3491 && b.col == 4),
        "collector-nearest Tier-1 hop AS3491 must be preserved beyond AS3356"
    );

    // Every intermediate waypoint on a multi-column edge must clear every real box in that column.
    for edge in &g.edges {
        for &(wp_col, wp_slot) in &edge.waypoints {
            for b in &g.boxes {
                if b.col == wp_col {
                    assert!(
                        (b.y_slot - wp_slot).abs() >= 0.60,
                        "waypoint at col={wp_col} slot={wp_slot:.2} collides with box AS{} at \
                         slot={:.2}",
                        b.asn,
                        b.y_slot
                    );
                }
            }
        }
    }

    let svg = report::bgp_graph::render_svg(&g);
    for mult in [3, 4, 5, 9] {
        let entity = format!("&#xD7;{mult}</text>");
        assert!(svg.contains(&entity), "missing SVG badge {entity}:\n{svg}");
    }

    let mermaid = report::bgp_graph::render_mermaid(&g);
    for mult in [3, 4, 5, 9] {
        let edge = format!("-. \"×{mult}\" .->");
        assert!(mermaid.contains(&edge), "missing Mermaid badge {edge}");
    }
    assert!(
        mermaid.contains("linkStyle "),
        "Mermaid highlights main vs secondary edges via linkStyle"
    );
}

#[test]
fn single_transit_fork_centers_origin_vertically_between_two_branches() {
    let mut names = BTreeMap::new();
    names.insert(209642, "Mejiro Network".to_owned());
    names.insert(38008, "Aperture Science".to_owned());

    let fam = FamilyBgp {
        address: "198.51.100.8".parse().expect("ip"),
        announced: true,
        prefix: Some("198.51.100.0/24".to_owned()),
        prefix_base: Some("198.51.100.0".parse().expect("base")),
        prefix_len: Some(24),
        origin_asn: Some(209642),
        origins: vec![OriginSummary {
            asn: 209642,
            peers: 127,
        }],
        rpki: RpkiStatus::Valid,
        total_peers: 127,
        distinct_paths: 2,
        upstreams: vec![UpstreamShare {
            asn: 38008,
            peers: 127,
        }],
        paths: vec![
            path(&[3356, 38008, 209642, 209642], 70, true),
            path(&[174, 38008, 209642, 209642], 57, false),
        ],
        asn_names: names,
        diagram_url: None,
    };

    let g = report::bgp_graph::layout(&fam, None).expect("layout");
    let origin_box = g.boxes.iter().find(|b| b.asn == 209642).expect("origin");
    let transit_box = g.boxes.iter().find(|b| b.asn == 38008).expect("transit");
    assert_eq!(origin_box.col, 0);
    assert_eq!(transit_box.col, 2, "×2 prepend places AS38008 at column 2");
    assert!(
        origin_box.y_slot > 0.2 && origin_box.y_slot < 0.8,
        "origin y_slot={} must sit between row 0.0 and row 1.0 instead of hugging the bottom",
        origin_box.y_slot
    );
    let svg = report::bgp_graph::render_svg(&g);
    assert!(
        svg.contains("Aperture Science</text>"),
        "16-char subtitle fits untruncated inside 18-column display budget:\n{svg}"
    );
}

#[test]
fn four_column_wide_glyph_dag_escapes_xml_and_mermaid_labels_cleanly() {
    let origin_label = "\u{8d77}\u{6e90} & <Cloud>";
    let transit_label = "\u{6c47}\u{805a} \"Transit\"";
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
            path(&[3356, 50010, 4809, 64500], 20, true),
            path(&[1299, 50010, 4809, 64500], 10, false),
            path(&[3356, 50010, 9929, 64500], 12, false),
            path(&[1299, 50010, 9929, 64500], 8, false),
        ],
        asn_names: names,
        diagram_url: None,
    };

    let g = report::bgp_graph::layout(&fam, Some(report::bgp_graph::COMPACT_MAX_PATHS))
        .expect("layout");
    assert_eq!(g.max_col, 3);

    let svg = report::bgp_graph::render_svg(&g);
    assert!(svg.contains("&amp; &lt;Cloud&gt;"));
    assert!(svg.contains("&quot;Transit&quot;"));

    let mermaid = report::bgp_graph::render_mermaid(&g);
    assert!(mermaid.contains("'Transit'"));
}

#[test]
fn deep_seven_layer_hierarchy_preserves_every_clean_hop_layer_in_svg() {
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
        diagram_url: None,
    };

    let g = report::bgp_graph::layout(&fam, None).expect("layout");
    assert_eq!(g.max_col, 6, "preserves all 7 clean hop layers (0..=6)");

    let svg = report::bgp_graph::render_svg(&g);
    assert!(svg.contains(">origin</text>"));
    assert!(svg.contains(">6</text>"));
    assert!(!svg.contains("2001:db8:dead"));
}
