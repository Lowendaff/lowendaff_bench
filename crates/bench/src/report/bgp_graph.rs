//! BGP `AS_PATH` topology layout and multi-format renderers:
//! - two-pass crossing-free Sugiyama/tree layout ported from `peer.as` (`PathGraph.svelte`);
//! - self-contained inline-styled SVG renderer (for `--format json`);
//! - GitHub-Flavored Markdown `mermaid` `flowchart LR` renderer (for `--format markdown`);
//! - Unicode 2D rounded-box and orthogonal-wire renderer with narrow-TTY tree fallback (for
//!   `--format text`).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::collections::btree_map::Entry;
use std::fmt::Write;

use plugin_network::bgp::BgpPath;
use plugin_network::bgp::FamilyBgp;
use plugin_network::bgp::asns;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

/// Maximum distinct paths included in Markdown and terminal diagrams so wide anycast networks
/// stay readable on screen. SVG keeps every path in `FamilyBgp::paths`.
pub const COMPACT_MAX_PATHS: usize = 14;

/// Maximum hops kept when a prefix has no Tier-1 transit path at all (e.g. pure IXP peering).
const FALLBACK_MAX_HOPS: usize = 5;

/// One positioned `(asn, column)` node box in [`PositionedGraph`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphBox {
    /// Autonomous System number.
    pub asn: u32,
    /// Hop distance column from origin (`0` = origin on the left, increasing rightward).
    pub col: usize,
    /// Quantized integer row (`0..=max_row`, top to bottom).
    pub row: usize,
    /// Whether `asn` is an origin of the prefix.
    pub is_origin: bool,
    /// Whether `asn` is a global Tier-1 backbone (`★`).
    pub is_tier1: bool,
    /// Concise display name, or empty when unknown.
    pub name: String,
}

/// One directed link from `from` (closer to origin, left) to `to` (closer to Tier-1, right).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    /// Downstream ASN (closer to origin).
    pub from_asn: u32,
    /// Column of `from_asn`.
    pub from_col: usize,
    /// Row of `from_asn`.
    pub from_row: usize,
    /// Upstream ASN (closer to Tier-1).
    pub to_asn: u32,
    /// Column of `to_asn` (`> from_col`).
    pub to_col: usize,
    /// Row of `to_asn`.
    pub to_row: usize,
    /// Aggregated RIS peer count across paths traversing this edge.
    pub weight: u32,
    /// Column span (`to_col - from_col`); values `>= 2` represent collapsed AS-PATH prepending.
    pub span: usize,
    /// Whether this edge belongs to the highest-peer main path.
    pub is_main: bool,
}

/// Positioned layered graph shared by the SVG, Mermaid, and terminal renderers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionedGraph {
    /// Rightmost column index (`0` when only the origin box exists).
    pub max_col: usize,
    /// Bottommost row index.
    pub max_row: usize,
    /// Positioned boxes ordered by `(col, row, asn)`.
    pub boxes: Vec<GraphBox>,
    /// Positioned edges ordered with non-main edges first and main-path edges last (top layer).
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeKey {
    from_asn: u32,
    from_col: usize,
    to_asn: u32,
    to_col: usize,
}

#[derive(Debug, Clone, Copy)]
struct HopCell {
    asn: u32,
    col: usize,
}

/// Computes the two-pass crossing-free layout for `family`, or `None` when `family` is not
/// announced or has no paths.
#[must_use]
pub fn layout(family: &FamilyBgp, max_paths: Option<usize>) -> Option<PositionedGraph> {
    if !family.announced || family.paths.is_empty() {
        return None;
    }

    // Prefer Tier-1-anchored paths (matching peer.as); fall back to all paths when none transits
    // a Tier-1 backbone (such as IXP-only regional routes or local smoke fixtures).
    let tier1_paths: Vec<&BgpPath> = family
        .paths
        .iter()
        .filter(|p| !p.asns.is_empty() && p.asns.iter().any(|a| asns::is_tier1(*a)))
        .collect();
    let use_tier1 = !tier1_paths.is_empty();
    let mut selected: Vec<&BgpPath> = if use_tier1 {
        tier1_paths
    } else {
        family.paths.iter().filter(|p| !p.asns.is_empty()).collect()
    };
    if let Some(limit) = max_paths {
        selected.truncate(limit.max(1));
    }
    if selected.is_empty() {
        return None;
    }

    let mut origin_set = BTreeSet::new();
    if !family.origins.is_empty() {
        for o in &family.origins {
            origin_set.insert(o.asn);
        }
    } else if let Some(origin) = family.origin_asn {
        origin_set.insert(origin);
    } else if let Some(&last) = selected[0].asns.last() {
        origin_set.insert(last);
    }

    let mut cell_cols: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    let mut edge_weights: BTreeMap<EdgeKey, u32> = BTreeMap::new();

    for path in &selected {
        let seq = collapse_prepends(path, use_tier1);
        for hop in &seq {
            cell_cols.entry(hop.asn).or_default().insert(hop.col);
        }
        for pair in seq.windows(2) {
            let lo = pair[0];
            let hi = pair[1];
            let key = EdgeKey {
                from_asn: lo.asn,
                from_col: lo.col,
                to_asn: hi.asn,
                to_col: hi.col,
            };
            let entry = edge_weights.entry(key).or_insert(0);
            *entry = entry.saturating_add(path.peers.max(1));
        }
    }

    if cell_cols.is_empty() {
        return None;
    }

    // Elect the main path: highest peer count, then `is_best`, then shortest clean path.
    let best = selected.iter().copied().max_by(|a, b| {
        a.peers
            .cmp(&b.peers)
            .then_with(|| a.is_best.cmp(&b.is_best))
            .then_with(|| b.asns.len().cmp(&a.asns.len()))
    });
    let mut main_edges = BTreeSet::new();
    if let Some(bp) = best {
        let seq = collapse_prepends(bp, use_tier1);
        for pair in seq.windows(2) {
            main_edges.insert(EdgeKey {
                from_asn: pair[0].asn,
                from_col: pair[0].col,
                to_asn: pair[1].asn,
                to_col: pair[1].col,
            });
        }
    }

    let all_asns: Vec<u32> = cell_cols.keys().copied().collect();
    let mut max_col = 0;
    for cols in cell_cols.values() {
        if let Some(&c) = cols.iter().next_back() {
            max_col = max_col.max(c);
        }
    }

    // Pass 1: Upstream subtree leaf-count sizing and centered-trunk DFS from origin(s).
    let mut children_of: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for key in edge_weights.keys() {
        children_of
            .entry(key.from_asn)
            .or_default()
            .insert(key.to_asn);
    }

    let mut subtree_size: BTreeMap<u32, usize> = BTreeMap::new();
    for &asn in &all_asns {
        compute_size(asn, &children_of, &mut subtree_size);
    }

    let mut roots: Vec<u32> = all_asns
        .iter()
        .copied()
        .filter(|asn| origin_set.contains(asn))
        .collect();
    if roots.is_empty()
        && let Some(&fallback_root) = all_asns
            .iter()
            .min_by_key(|&&a| cell_cols[&a].iter().next().copied().unwrap_or(usize::MAX))
    {
        roots.push(fallback_root);
    }

    let mut ord: BTreeMap<u32, f64> = BTreeMap::new();
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut leaf_counter: usize = 0;
    for root in roots {
        dfs_order(
            root,
            &children_of,
            &subtree_size,
            &mut seen,
            &mut ord,
            &mut leaf_counter,
        );
    }
    for &asn in &all_asns {
        if let Entry::Vacant(vacant) = ord.entry(asn) {
            vacant.insert(leaf_counter as f64);
            leaf_counter += 1;
        }
    }

    // Pass 2: Sparsest-column-first integer row quantization with prepend-corridor reservation.
    let mut col_members: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for &asn in &all_asns {
        for &col in &cell_cols[&asn] {
            col_members.entry(col).or_default().push(asn);
        }
    }
    let max_count = col_members.values().map(Vec::len).max().unwrap_or(1).max(1);

    // Record multi-column span edges (`to_col > from_col + 1`) so intermediate columns keep the
    // source ASN's horizontal corridor clear of unrelated boxes.
    let mut span_corridors_of: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for key in edge_weights.keys() {
        if key.to_col > key.from_col + 1 {
            let entry = span_corridors_of.entry(key.from_asn).or_default();
            for mid_col in (key.from_col + 1)..key.to_col {
                if !entry.contains(&mid_col) {
                    entry.push(mid_col);
                }
            }
        }
    }

    let mut cols_by_density: Vec<usize> = col_members.keys().copied().collect();
    cols_by_density.sort_by_key(|&c| (col_members[&c].len(), c));

    let mut row_of: BTreeMap<u32, usize> = BTreeMap::new();
    let mut col_used_rows: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();

    for col in cols_by_density {
        let mut members = col_members[&col].clone();
        members.sort_by(|a, b| ord[a].total_cmp(&ord[b]).then_with(|| a.cmp(b)));

        let start = max_count.saturating_sub(members.len()).div_ceil(2);
        let mut taken = col_used_rows.get(&col).cloned().unwrap_or_default();
        for &a in &members {
            if let Some(&r) = row_of.get(&a) {
                taken.insert(r);
            }
        }
        let mut ptr = start;
        for a in members {
            if let Some(&existing) = row_of.get(&a) {
                col_used_rows.entry(col).or_default().insert(existing);
                ptr = ptr.max(existing + 1);
                continue;
            }
            let corridors = span_corridors_of.get(&a);
            while taken.contains(&ptr)
                || corridors.is_some_and(|mids| {
                    mids.iter()
                        .any(|mc| col_used_rows.get(mc).is_some_and(|u| u.contains(&ptr)))
                })
            {
                ptr += 1;
            }
            row_of.insert(a, ptr);
            taken.insert(ptr);
            col_used_rows.entry(col).or_default().insert(ptr);
            if let Some(mids) = corridors {
                for &mc in mids {
                    col_used_rows.entry(mc).or_default().insert(ptr);
                }
            }
            ptr += 1;
        }
    }

    let max_row = row_of.values().copied().max().unwrap_or(0);
    let mut boxes = Vec::new();
    for &asn in &all_asns {
        let row = row_of[&asn];
        let is_origin = origin_set.contains(&asn);
        let is_tier1 = asns::is_tier1(asn);
        let name = family
            .asn_names
            .get(&asn)
            .cloned()
            .or_else(|| asns::known_name(asn).map(str::to_owned))
            .unwrap_or_default();
        for &col in &cell_cols[&asn] {
            boxes.push(GraphBox {
                asn,
                col,
                row,
                is_origin,
                is_tier1,
                name: name.clone(),
            });
        }
    }
    boxes.sort_by_key(|b| (b.col, b.row, b.asn));

    let mut edges = Vec::with_capacity(edge_weights.len());
    for (key, weight) in edge_weights {
        let (Some(&from_row), Some(&to_row)) = (row_of.get(&key.from_asn), row_of.get(&key.to_asn))
        else {
            continue;
        };
        edges.push(GraphEdge {
            from_asn: key.from_asn,
            from_col: key.from_col,
            from_row,
            to_asn: key.to_asn,
            to_col: key.to_col,
            to_row,
            weight,
            span: key.to_col.saturating_sub(key.from_col),
            is_main: main_edges.contains(&key),
        });
    }
    // Draw main-path edges last so they sit above secondary edges without occlusion.
    edges.sort_by_key(|e| (e.is_main, e.from_col, e.from_row, e.to_row));

    Some(PositionedGraph {
        max_col,
        max_row,
        boxes,
        edges,
    })
}

fn collapse_prepends(path: &BgpPath, use_tier1: bool) -> Vec<HopCell> {
    let source = if path.asns_raw.is_empty() {
        &path.asns
    } else {
        &path.asns_raw
    };
    let trimmed = if use_tier1 {
        asns::trunc_to_tier1(source)
    } else if source.len() > FALLBACK_MAX_HOPS {
        source[source.len() - FALLBACK_MAX_HOPS..].to_vec()
    } else {
        source.clone()
    };
    let mut seq: Vec<HopCell> = Vec::with_capacity(trimmed.len());
    for (d, &asn) in trimmed.iter().rev().enumerate() {
        if seq.last().is_some_and(|prev| prev.asn == asn) {
            continue;
        }
        seq.push(HopCell { asn, col: d });
    }
    seq
}

fn compute_size(
    asn: u32,
    children_of: &BTreeMap<u32, BTreeSet<u32>>,
    memo: &mut BTreeMap<u32, usize>,
) -> usize {
    if let Some(&cached) = memo.get(&asn) {
        return cached;
    }
    memo.insert(asn, 1);
    let Some(children) = children_of.get(&asn) else {
        return 1;
    };
    if children.is_empty() {
        return 1;
    }
    let mut sum = 0;
    for &child in children {
        sum += compute_size(child, children_of, memo);
    }
    let val = sum.max(1);
    memo.insert(asn, val);
    val
}

fn dfs_order(
    asn: u32,
    children_of: &BTreeMap<u32, BTreeSet<u32>>,
    subtree_size: &BTreeMap<u32, usize>,
    seen: &mut BTreeSet<u32>,
    ord: &mut BTreeMap<u32, f64>,
    leaf_counter: &mut usize,
) -> f64 {
    if seen.contains(&asn) {
        return ord.get(&asn).copied().unwrap_or(0.0);
    }
    seen.insert(asn);
    let all: Vec<u32> = children_of
        .get(&asn)
        .map(|set| set.iter().copied().collect())
        .unwrap_or_default();
    if all.is_empty() {
        let val = *leaf_counter as f64;
        *leaf_counter += 1;
        ord.insert(asn, val);
        return val;
    }

    // Sort unvisited children largest-subtree-first and alternate front/back so the dominant
    // trunk sits in the vertical center while smaller leaves fan out to top and bottom.
    let mut unvisited: Vec<u32> = all.iter().copied().filter(|k| !seen.contains(k)).collect();
    unvisited.sort_by(|a, b| subtree_size[b].cmp(&subtree_size[a]).then_with(|| a.cmp(b)));
    let mut centered: VecDeque<u32> = VecDeque::with_capacity(unvisited.len());
    for (idx, child) in unvisited.into_iter().enumerate() {
        if idx % 2 == 1 {
            centered.push_back(child);
        } else {
            centered.push_front(child);
        }
    }

    let mut values = Vec::with_capacity(all.len());
    for &child in &centered {
        values.push(dfs_order(
            child,
            children_of,
            subtree_size,
            seen,
            ord,
            leaf_counter,
        ));
    }
    for &child in &all {
        if !centered.contains(&child)
            && let Some(&existing) = ord.get(&child)
        {
            values.push(existing);
        }
    }

    let mean = if values.is_empty() {
        let v = *leaf_counter as f64;
        *leaf_counter += 1;
        v
    } else {
        values.iter().sum::<f64>() / (values.len() as f64)
    };
    ord.insert(asn, mean);
    mean
}

// ---------------------------------------------------------------------------
// 1. Self-contained inline-styled SVG renderer (`--format json`)
// ---------------------------------------------------------------------------

const SVG_NW: f64 = 120.0;
const SVG_NH: f64 = 34.0;
const SVG_COLG: f64 = 56.0;
const SVG_ROWG: f64 = 14.0;
const SVG_HEAD: f64 = 26.0;

/// Renders `graph` as a standalone UTF-8 SVG document with baked inline styles, matching the
/// visual geometry of `peer.as`.
#[must_use]
pub fn render_svg(graph: &PositionedGraph) -> String {
    let col_p = SVG_NW + SVG_COLG;
    let row_p = SVG_NH + SVG_ROWG;
    let cx = |col: usize| SVG_COLG + (col as f64) * col_p + SVG_NW / 2.0;
    let cy = |row: usize| SVG_HEAD + SVG_ROWG / 2.0 + (row as f64) * row_p + SVG_NH / 2.0;

    let width = ((graph.max_col + 1) as f64) * col_p + SVG_COLG;
    let height = SVG_HEAD + SVG_ROWG + ((graph.max_row + 1) as f64) * row_p;
    let w_int = width.round() as u32;
    let h_int = height.round() as u32;

    let mut out = String::with_capacity(2048);
    let _ = writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w_int} {h_int}\" \
         width=\"{w_int}\" height=\"{h_int}\">"
    );
    let _ = writeln!(
        out,
        "  <rect width=\"{w_int}\" height=\"{h_int}\" rx=\"8\" fill=\"#0b1220\"/>"
    );

    // Distance axis lines and labels (`origin`, `1`, `2`, ...).
    for col in 0..=graph.max_col {
        let x = cx(col);
        let label = if col == 0 {
            "origin".to_owned()
        } else {
            col.to_string()
        };
        let _ = writeln!(
            out,
            "  <line x1=\"{x:.1}\" y1=\"18\" x2=\"{x:.1}\" y2=\"{h_int}\" \
             stroke=\"#1e293b\" stroke-width=\"1\" opacity=\"0.8\"/>"
        );
        let _ = writeln!(
            out,
            "  <text x=\"{x:.1}\" y=\"13\" fill=\"#94a3b8\" font-family=\"ui-monospace,monospace\" \
             font-size=\"10\" text-anchor=\"middle\">{label}</text>"
        );
    }

    // Cubic Bezier edges.
    for edge in &graph.edges {
        let x1 = cx(edge.from_col) + SVG_NW / 2.0;
        let y1 = cy(edge.from_row);
        let x2 = cx(edge.to_col) - SVG_NW / 2.0;
        let y2 = cy(edge.to_row);
        let mx = (x1 + x2) / 2.0;
        let my = (y1 + y2) / 2.0;
        let sw = (1.0 + (f64::from(edge.weight) + 1.0).log2() / 2.0).min(4.5);
        let (stroke, opacity) = if edge.is_main {
            ("#38bdf8", "0.9")
        } else {
            ("#64748b", "0.45")
        };
        let dash = if edge.span >= 2 {
            " stroke-dasharray=\"5 4\""
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "  <path d=\"M{x1:.1},{y1:.1} C{mx:.1},{y1:.1} {mx:.1},{y2:.1} {x2:.1},{y2:.1}\" \
             fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{sw:.2}\" opacity=\"{opacity}\"{dash}/>"
        );
        if edge.span >= 2 {
            let ly = my - 5.0;
            let _ = writeln!(
                out,
                "  <text x=\"{mx:.1}\" y=\"{ly:.1}\" fill=\"#94a3b8\" \
                 font-family=\"ui-monospace,monospace\" font-size=\"10\" font-weight=\"700\" \
                 text-anchor=\"middle\">&#xD7;{}</text>",
                edge.span
            );
        }
    }

    // ASN boxes.
    for b in &graph.boxes {
        let x = cx(b.col);
        let y = cy(b.row);
        let rx = x - SVG_NW / 2.0;
        let ry = y - SVG_NH / 2.0;
        let (fill, stroke, sw, asn_fill) = if b.is_origin {
            ("#0c4a6e", "#38bdf8", "2", "#f8fafc")
        } else if b.is_tier1 {
            ("#0f172a", "#10b981", "2.4", "#34d399")
        } else {
            ("#0f172a", "#64748b", "1.4", "#f1f5f9")
        };
        let star = if b.is_tier1 { " &#x2605;" } else { "" };
        let short_name = truncate_chars(&b.name, 15);
        let _ = writeln!(out, "  <g>");
        let _ = writeln!(
            out,
            "    <rect x=\"{rx:.1}\" y=\"{ry:.1}\" width=\"{SVG_NW:.0}\" height=\"{SVG_NH:.0}\" \
             rx=\"5\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{sw}\"/>"
        );
        let asn_y = if short_name.is_empty() {
            y + 1.0
        } else {
            y - 3.0
        };
        let _ = writeln!(
            out,
            "    <text x=\"{x:.1}\" y=\"{asn_y:.1}\" fill=\"{asn_fill}\" \
             font-family=\"ui-monospace,monospace\" font-size=\"11\" font-weight=\"700\" \
             text-anchor=\"middle\" dominant-baseline=\"middle\">AS{}{star}</text>",
            b.asn
        );
        if !short_name.is_empty() {
            let ny = y + 10.0;
            let _ = writeln!(
                out,
                "    <text x=\"{x:.1}\" y=\"{ny:.1}\" fill=\"#94a3b8\" \
                 font-family=\"system-ui,sans-serif\" font-size=\"10\" \
                 text-anchor=\"middle\">{}</text>",
                escape_xml(&short_name)
            );
        }
        let _ = writeln!(out, "  </g>");
    }

    out.push_str("</svg>");
    out
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 2. GitHub-Flavored Markdown Mermaid renderer (`--format markdown`)
// ---------------------------------------------------------------------------

/// Renders `graph` as a Mermaid `flowchart LR` source block (without surrounding backticks) that
/// GitHub, GitLab, Obsidian, and Notion render directly as an interactive vector diagram.
#[must_use]
pub fn render_mermaid(graph: &PositionedGraph) -> String {
    let mut out = String::new();
    out.push_str("flowchart LR\n");
    out.push_str("  classDef origin fill:#0c4a6e,stroke:#38bdf8,stroke-width:2px,color:#f8fafc\n");
    out.push_str("  classDef tier1 fill:#064e3b,stroke:#10b981,stroke-width:2px,color:#ecfdf5\n");
    out.push_str(
        "  classDef transit fill:#0f172a,stroke:#64748b,stroke-width:1.5px,color:#f1f5f9\n",
    );

    for b in &graph.boxes {
        let id = format!("n_{}_{}", b.asn, b.col);
        let star = if b.is_tier1 { " ★" } else { "" };
        let short = truncate_chars(&b.name, 16);
        let label = if short.is_empty() {
            format!("AS{}{star}", b.asn)
        } else {
            format!("AS{}{star} · {}", b.asn, sanitize_mermaid_label(&short))
        };
        let cls = if b.is_origin {
            "origin"
        } else if b.is_tier1 {
            "tier1"
        } else {
            "transit"
        };
        let _ = writeln!(out, "  {id}[\"{label}\"]:::{cls}");
    }

    for e in &graph.edges {
        let src = format!("n_{}_{}", e.from_asn, e.from_col);
        let dst = format!("n_{}_{}", e.to_asn, e.to_col);
        if e.span >= 2 {
            if e.is_main {
                let _ = writeln!(out, "  {src} == \"×{}\" ==> {dst}", e.span);
            } else {
                let _ = writeln!(out, "  {src} -. \"×{}\" .-> {dst}", e.span);
            }
        } else if e.is_main {
            let _ = writeln!(out, "  {src} ==> {dst}");
        } else {
            let _ = writeln!(out, "  {src} --> {dst}");
        }
    }

    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn sanitize_mermaid_label(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '"' | '`' | '\\' => '\'',
            '\r' | '\n' => ' ',
            other => other,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 3. Terminal Unicode 2D box-and-wire renderer (`--format text`)
// ---------------------------------------------------------------------------

const TERM_INNER_W: usize = 14;
const TERM_BOX_W: usize = TERM_INNER_W + 2; // 16 display columns
const TERM_ROW_H: usize = 4;

const BIT_LEFT: u8 = 1;
const BIT_RIGHT: u8 = 2;
const BIT_UP: u8 = 4;
const BIT_DOWN: u8 = 8;
const BIT_HEAVY: u8 = 16;
const BIT_DASH: u8 = 32;

/// Renders `graph` as Unicode rounded boxes connected by orthogonal wires (`═▸` main path,
/// `─▸` secondary paths, `┄×N▸` collapsed prepends). Automatically falls back to a compact
/// indented Unicode tree when `max_width` is narrower than the horizontal grid.
#[must_use]
pub fn render_terminal(
    graph: &PositionedGraph,
    indent: &str,
    max_width: Option<u16>,
) -> Vec<String> {
    // Compute per-gap vertical track assignments (`gap = to_col - 1`).
    let mut gap_tracks: Vec<Vec<usize>> = Vec::with_capacity(graph.max_col);
    for g in 0..graph.max_col {
        let mut sources = BTreeSet::new();
        for e in &graph.edges {
            if e.to_col == g + 1 && e.from_row != e.to_row {
                sources.insert(e.from_row);
            }
        }
        gap_tracks.push(sources.into_iter().collect());
    }

    let mut col_x = vec![0_usize; graph.max_col + 1];
    let mut gap_x = vec![0_usize; graph.max_col];
    let mut cur_x = 0_usize;
    for c in 0..=graph.max_col {
        col_x[c] = cur_x;
        cur_x += TERM_BOX_W;
        if c < graph.max_col {
            gap_x[c] = cur_x;
            let gap_w = (gap_tracks[c].len() + 4).max(6);
            cur_x += gap_w;
        }
    }
    let total_w = cur_x;

    if let Some(limit) = max_width
        && indent.width() + total_w > usize::from(limit)
        && graph.max_col > 0
    {
        return render_tree_fallback(graph, indent);
    }

    let total_h = 1 + (graph.max_row + 1) * TERM_ROW_H;
    let mut chars: Vec<Vec<char>> = vec![vec![' '; total_w]; total_h];
    // `wide_cont[y][x]` marks the trailing column of a 2-column-wide glyph so serialization
    // emits the glyph once while keeping exact display-column coordinates across the grid.
    let mut wide_cont: Vec<Vec<bool>> = vec![vec![false; total_w]; total_h];
    let mut wires: Vec<Vec<u8>> = vec![vec![0; total_w]; total_h];

    // 1. Column distance header row (`y = 0`).
    for (col_idx, &cx_val) in col_x.iter().enumerate() {
        let label = if col_idx == 0 {
            "origin".to_owned()
        } else {
            col_idx.to_string()
        };
        let pad_left = TERM_BOX_W.saturating_sub(label.width()) / 2;
        put_str(&mut chars[0], &mut wide_cont[0], cx_val + pad_left, &label);
    }

    // 2. Wire bitmasks across inter-column gaps and empty prepend corridors.
    let y_port = |row: usize| 2 + row * TERM_ROW_H;
    for e in &graph.edges {
        let x_start = col_x[e.from_col] + TERM_BOX_W;
        let x_end = col_x[e.to_col].saturating_sub(1);
        if x_start > x_end {
            continue;
        }
        let style = if e.is_main {
            BIT_HEAVY
        } else if e.span >= 2 {
            BIT_DASH
        } else {
            0
        };
        let y1 = y_port(e.from_row);
        let y2 = y_port(e.to_row);

        if y1 == y2 {
            for cell in &mut wires[y1][x_start..x_end] {
                *cell |= BIT_LEFT | BIT_RIGHT | style;
            }
        } else {
            let g = e.to_col - 1;
            let track_idx = gap_tracks[g].binary_search(&e.from_row).unwrap_or(0);
            let tx = (gap_x[g] + 1 + track_idx).min(x_end.saturating_sub(1));

            for cell in &mut wires[y1][x_start..tx] {
                *cell |= BIT_LEFT | BIT_RIGHT | style;
            }
            let turn_from = if y2 > y1 { BIT_DOWN } else { BIT_UP };
            wires[y1][tx] |= BIT_LEFT | turn_from | style;

            let (ylo, yhi) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
            for row_wires in &mut wires[(ylo + 1)..yhi] {
                row_wires[tx] |= BIT_UP | BIT_DOWN | style;
            }

            let turn_to = if y2 > y1 { BIT_UP } else { BIT_DOWN };
            wires[y2][tx] |= turn_to | BIT_RIGHT | style;
            for cell in &mut wires[y2][(tx + 1)..x_end] {
                *cell |= BIT_LEFT | BIT_RIGHT | style;
            }
        }
    }

    for y in 1..total_h {
        for x in 0..total_w {
            let mask = wires[y][x];
            if mask != 0 {
                chars[y][x] = wire_glyph(mask);
            }
        }
    }

    // Place arrowheads and prepend `×N` badges on top of wires.
    for e in &graph.edges {
        let x_end = col_x[e.to_col].saturating_sub(1);
        let y2 = y_port(e.to_row);
        chars[y2][x_end] = '▸';
        if e.span >= 2 {
            let x_start = col_x[e.from_col] + TERM_BOX_W;
            let badge = format!("×{}", e.span);
            if x_start + badge.width() < x_end {
                let bx = x_start + 1;
                put_str(
                    &mut chars[y_port(e.from_row)],
                    &mut wide_cont[y_port(e.from_row)],
                    bx,
                    &badge,
                );
            }
        }
    }

    // 3. Draw rounded ASN node boxes (`╭─╮│╰─╯`).
    let top_border = format!("╭{}╮", "─".repeat(TERM_INNER_W));
    let bot_border = format!("╰{}╯", "─".repeat(TERM_INNER_W));
    for b in &graph.boxes {
        let x = col_x[b.col];
        let y0 = 1 + b.row * TERM_ROW_H;
        let star = if b.is_tier1 { " ★" } else { "" };
        let line1 = fit_cell(&format!(" AS{}{star}", b.asn), TERM_INNER_W);
        let line2 = fit_cell(&format!(" {}", b.name), TERM_INNER_W);

        put_str(&mut chars[y0], &mut wide_cont[y0], x, &top_border);
        put_str(
            &mut chars[y0 + 1],
            &mut wide_cont[y0 + 1],
            x,
            &format!("│{line1}│"),
        );
        put_str(
            &mut chars[y0 + 2],
            &mut wide_cont[y0 + 2],
            x,
            &format!("│{line2}│"),
        );
        put_str(&mut chars[y0 + 3], &mut wide_cont[y0 + 3], x, &bot_border);
    }

    let mut lines = Vec::with_capacity(total_h);
    for y in 0..total_h {
        let mut row_str = String::with_capacity(indent.len() + total_w);
        row_str.push_str(indent);
        for x in 0..total_w {
            if !wide_cont[y][x] {
                row_str.push(chars[y][x]);
            }
        }
        let trimmed = row_str.trim_end().to_owned();
        lines.push(trimmed);
    }
    lines
}

fn wire_glyph(mask: u8) -> char {
    let dirs = mask & 0x0f;
    let heavy = (mask & BIT_HEAVY) != 0;
    let dash = (mask & BIT_DASH) != 0;
    match dirs {
        d if d == (BIT_LEFT | BIT_RIGHT) => {
            if heavy {
                '═'
            } else if dash {
                '┄'
            } else {
                '─'
            }
        }
        d if d == (BIT_UP | BIT_DOWN) => '│',
        d if d == (BIT_DOWN | BIT_RIGHT) => '╭',
        d if d == (BIT_DOWN | BIT_LEFT) => '╮',
        d if d == (BIT_UP | BIT_RIGHT) => '╰',
        d if d == (BIT_UP | BIT_LEFT) => '╯',
        d if d == (BIT_UP | BIT_DOWN | BIT_RIGHT) => '├',
        d if d == (BIT_UP | BIT_DOWN | BIT_LEFT) => '┤',
        d if d == (BIT_LEFT | BIT_RIGHT | BIT_DOWN) => '┬',
        d if d == (BIT_LEFT | BIT_RIGHT | BIT_UP) => '┴',
        _ => '┼',
    }
}

fn put_str(row: &mut [char], cont: &mut [bool], start_x: usize, text: &str) {
    let mut x = start_x;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w == 0 || x + w > row.len() {
            continue;
        }
        row[x] = ch;
        cont[x] = false;
        if w == 2 {
            row[x + 1] = ' ';
            cont[x + 1] = true;
        }
        x += w;
    }
}

fn fit_cell(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    if used < width {
        out.push_str(&" ".repeat(width - used));
    }
    out
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Compact Unicode tree fallback when the terminal window is narrower than the multi-column grid.
fn render_tree_fallback(graph: &PositionedGraph, indent: &str) -> Vec<String> {
    let mut box_map: BTreeMap<(u32, usize), &GraphBox> = BTreeMap::new();
    for b in &graph.boxes {
        box_map.insert((b.asn, b.col), b);
    }
    let mut children: BTreeMap<(u32, usize), Vec<&GraphEdge>> = BTreeMap::new();
    for e in &graph.edges {
        children
            .entry((e.from_asn, e.from_col))
            .or_default()
            .push(e);
    }
    for list in children.values_mut() {
        list.sort_by_key(|e| (e.to_row, e.to_asn));
    }

    let mut roots: Vec<&GraphBox> = graph.boxes.iter().filter(|b| b.col == 0).collect();
    roots.sort_by_key(|b| (b.row, b.asn));

    let mut lines = Vec::new();
    let mut visited = BTreeSet::new();
    for root in roots {
        lines.push(format!("{indent}{} (origin)", node_inline_label(root)));
        visited.insert((root.asn, root.col));
        emit_tree_children(
            (root.asn, root.col),
            indent,
            &box_map,
            &children,
            &mut visited,
            &mut lines,
        );
    }
    lines
}

fn emit_tree_children(
    node: (u32, usize),
    prefix: &str,
    box_map: &BTreeMap<(u32, usize), &GraphBox>,
    children: &BTreeMap<(u32, usize), Vec<&GraphEdge>>,
    visited: &mut BTreeSet<(u32, usize)>,
    lines: &mut Vec<String>,
) {
    let Some(edges) = children.get(&node) else {
        return;
    };
    let last_idx = edges.len().saturating_sub(1);
    for (idx, edge) in edges.iter().enumerate() {
        let is_last = idx == last_idx;
        let branch = if is_last { "└" } else { "├" };
        let wire = if edge.is_main {
            "══▸"
        } else {
            "──▸"
        };
        let span_note = if edge.span >= 2 {
            format!(" (×{})", edge.span)
        } else {
            String::new()
        };
        let target_key = (edge.to_asn, edge.to_col);
        let label = box_map
            .get(&target_key)
            .map_or_else(|| format!("AS{}", edge.to_asn), |b| node_inline_label(b));
        lines.push(format!("{prefix}{branch}{wire} {label}{span_note}"));
        if visited.insert(target_key) {
            let next_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });
            emit_tree_children(target_key, &next_prefix, box_map, children, visited, lines);
        }
    }
}

fn node_inline_label(b: &GraphBox) -> String {
    let star = if b.is_tier1 { " ★" } else { "" };
    if b.name.is_empty() {
        format!("AS{}{star}", b.asn)
    } else {
        format!("AS{}{star} {}", b.asn, b.name)
    }
}
