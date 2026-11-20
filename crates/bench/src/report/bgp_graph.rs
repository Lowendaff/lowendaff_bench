//! BGP `AS_PATH` topology layout and multi-format renderers:
//! - Sugiyama clean-hop layered layout with Tier-1-deduplicated path selection, centered-trunk
//!   DFS, barycenter crossing reduction, and neighbor-snapped monotonic row assignment;
//! - self-contained inline-styled SVG renderer (for `--format json`);
//! - GitHub-Flavored Markdown `mermaid` `flowchart LR` renderer (for `--format markdown`);
//! - two-port Unicode 2D rounded-box and orthogonal-wire renderer with compact-box scaling,
//!   multi-stage horizontal band wrapping for deep multi-layer topologies, and narrow-TTY tree
//!   fallback (for `--format text`).

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

/// Maximum distinct Tier-1-collapsed paths included in Markdown and terminal diagrams so wide
/// anycast networks stay readable on screen. SVG keeps every distinct collapsed path.
pub const COMPACT_MAX_PATHS: usize = 14;

/// Maximum clean hops kept when a prefix has no Tier-1 transit path at all (e.g. pure IXP peering).
const FALLBACK_MAX_HOPS: usize = 5;

/// Number of alternating left-to-right / right-to-left barycenter sweeps for crossing reduction.
const BARYCENTER_SWEEPS: usize = 3;

/// Maximum single-band canvas width when `max_width` is `None` (piped/file output) before wrapping
/// deep multi-layer graphs into continuation bands.
const UNBOUNDED_BAND_BUDGET: usize = 140;

/// Default band width used when wrapping piped/file output (`max_width == None`).
const UNBOUNDED_WRAP_TARGET: usize = 116;

/// One positioned `(asn, column)` node box in [`PositionedGraph`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphBox {
    /// Autonomous System number.
    pub asn: u32,
    /// Clean AS-hop distance column from origin (`0` = origin on the left, increasing rightward).
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
    /// Dominant `AS_PATH` prepend multiplicity on `from_asn` (`1` = normal, `>= 2` = `×N` prepend).
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

#[derive(Default)]
struct EdgeStats {
    weight: u32,
    prepend_weights: BTreeMap<usize, u32>,
}

#[derive(Debug, Clone, Copy)]
struct HopCell {
    asn: u32,
    repeat: usize,
}

struct CollapsedGroup {
    asns: Vec<u32>,
    peers: u32,
    has_best: bool,
    hop_repeats: Vec<BTreeMap<usize, u32>>,
}

type CellNeighborMap = BTreeMap<(u32, usize), Vec<((u32, usize), u32)>>;

/// Computes the crossing-reduced clean-hop layout for `family`, or `None` when `family` is not
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
    let candidates: Vec<&BgpPath> = if use_tier1 {
        tier1_paths
    } else {
        family.paths.iter().filter(|p| !p.asns.is_empty()).collect()
    };
    if candidates.is_empty() {
        return None;
    }

    // Deduplicate and sum peer counts by collapsed clean-hop sequence BEFORE applying `max_paths`
    // so collector-side duplicates (e.g. 200 RIS peers behind AS6939) do not crowd out multi-layer
    // backup upstreams.
    let mut grouped: BTreeMap<Vec<u32>, CollapsedGroup> = BTreeMap::new();
    for path in candidates {
        let seq = collapse_prepends(path, use_tier1);
        if seq.is_empty() {
            continue;
        }
        let key: Vec<u32> = seq.iter().map(|h| h.asn).collect();
        let w = path.peers.max(1);
        let entry = grouped
            .entry(key.clone())
            .or_insert_with(|| CollapsedGroup {
                asns: key,
                peers: 0,
                has_best: false,
                hop_repeats: vec![BTreeMap::new(); seq.len()],
            });
        entry.peers = entry.peers.saturating_add(w);
        entry.has_best |= path.is_best;
        for (idx, hop) in seq.iter().enumerate() {
            let rw = entry.hop_repeats[idx].entry(hop.repeat).or_insert(0);
            *rw = rw.saturating_add(w);
        }
    }

    let mut groups: Vec<CollapsedGroup> = grouped.into_values().collect();
    groups.sort_by(|a, b| {
        b.peers
            .cmp(&a.peers)
            .then_with(|| b.has_best.cmp(&a.has_best))
            .then_with(|| a.asns.len().cmp(&b.asns.len()))
            .then_with(|| a.asns.cmp(&b.asns))
    });
    if let Some(limit) = max_paths {
        groups.truncate(limit.max(1));
    }
    if groups.is_empty() {
        return None;
    }

    let mut origin_set = BTreeSet::new();
    if !family.origins.is_empty() {
        for o in &family.origins {
            origin_set.insert(o.asn);
        }
    } else if let Some(origin) = family.origin_asn {
        origin_set.insert(origin);
    } else {
        origin_set.insert(groups[0].asns[0]);
    }

    let mut cell_cols: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    let mut edge_stats: BTreeMap<EdgeKey, EdgeStats> = BTreeMap::new();

    for grp in &groups {
        for (col, &asn) in grp.asns.iter().enumerate() {
            cell_cols.entry(asn).or_default().insert(col);
        }
        for col in 0..grp.asns.len().saturating_sub(1) {
            let key = EdgeKey {
                from_asn: grp.asns[col],
                from_col: col,
                to_asn: grp.asns[col + 1],
                to_col: col + 1,
            };
            let stats = edge_stats.entry(key).or_default();
            stats.weight = stats.weight.saturating_add(grp.peers);
            for (&rep, &rw) in &grp.hop_repeats[col] {
                let pw = stats.prepend_weights.entry(rep).or_insert(0);
                *pw = pw.saturating_add(rw);
            }
        }
    }

    if cell_cols.is_empty() {
        return None;
    }

    // Elect the main path: `groups[0]` has the highest aggregated RIS peer count.
    let mut main_edges: BTreeMap<EdgeKey, usize> = BTreeMap::new();
    let best_grp = &groups[0];
    for col in 0..best_grp.asns.len().saturating_sub(1) {
        main_edges.insert(
            EdgeKey {
                from_asn: best_grp.asns[col],
                from_col: col,
                to_asn: best_grp.asns[col + 1],
                to_col: col + 1,
            },
            dominant_prepend(&best_grp.hop_repeats[col]),
        );
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
    for key in edge_stats.keys() {
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

    // Pass 2: Barycenter crossing reduction per `(asn, col)` layer, followed by densest-column
    // anchored monotonic integer row assignment.
    let mut col_members: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for &asn in &all_asns {
        for &col in &cell_cols[&asn] {
            col_members.entry(col).or_default().push(asn);
        }
    }
    for members in col_members.values_mut() {
        members.sort_by(|a, b| ord[a].total_cmp(&ord[b]).then_with(|| a.cmp(b)));
    }
    let max_count = col_members.values().map(Vec::len).max().unwrap_or(1).max(1);

    let mut in_adj: CellNeighborMap = BTreeMap::new();
    let mut out_adj: CellNeighborMap = BTreeMap::new();
    for (key, stats) in &edge_stats {
        in_adj
            .entry((key.to_asn, key.to_col))
            .or_default()
            .push(((key.from_asn, key.from_col), stats.weight));
        out_adj
            .entry((key.from_asn, key.from_col))
            .or_default()
            .push(((key.to_asn, key.to_col), stats.weight));
    }

    let mut cell_pos: BTreeMap<(u32, usize), f64> = BTreeMap::new();
    for (&col, members) in &col_members {
        let offset = (max_count.saturating_sub(members.len())) as f64 / 2.0;
        for (idx, &asn) in members.iter().enumerate() {
            cell_pos.insert((asn, col), offset + idx as f64);
        }
    }

    for sweep in 0..BARYCENTER_SWEEPS {
        if sweep % 2 == 0 {
            for col in 1..=max_col {
                refine_column_order(
                    col,
                    &in_adj,
                    &ord,
                    max_count,
                    &mut col_members,
                    &mut cell_pos,
                );
            }
        } else {
            for col in (0..max_col).rev() {
                refine_column_order(
                    col,
                    &out_adj,
                    &ord,
                    max_count,
                    &mut col_members,
                    &mut cell_pos,
                );
            }
        }
    }

    let anchor_col = (0..=max_col)
        .max_by_key(|&c| (col_members.get(&c).map_or(0, Vec::len), usize::MAX - c))
        .unwrap_or(0);

    let mut cell_row: BTreeMap<(u32, usize), usize> = BTreeMap::new();
    assign_column_rows(
        anchor_col,
        max_count,
        &col_members,
        &in_adj,
        &out_adj,
        &mut cell_row,
    );
    for col in (0..anchor_col).rev() {
        assign_column_rows(
            col,
            max_count,
            &col_members,
            &in_adj,
            &out_adj,
            &mut cell_row,
        );
    }
    for col in (anchor_col + 1)..=max_col {
        assign_column_rows(
            col,
            max_count,
            &col_members,
            &in_adj,
            &out_adj,
            &mut cell_row,
        );
    }

    let max_row = cell_row.values().copied().max().unwrap_or(0);
    let mut boxes = Vec::with_capacity(cell_row.len());
    for (&(asn, col), &row) in &cell_row {
        let is_origin = origin_set.contains(&asn);
        let is_tier1 = asns::is_tier1(asn);
        let name = family
            .asn_names
            .get(&asn)
            .cloned()
            .or_else(|| asns::known_name(asn).map(str::to_owned))
            .unwrap_or_default();
        boxes.push(GraphBox {
            asn,
            col,
            row,
            is_origin,
            is_tier1,
            name,
        });
    }
    boxes.sort_by_key(|b| (b.col, b.row, b.asn));

    let mut edges = Vec::with_capacity(edge_stats.len());
    for (key, stats) in edge_stats {
        let (Some(&from_row), Some(&to_row)) = (
            cell_row.get(&(key.from_asn, key.from_col)),
            cell_row.get(&(key.to_asn, key.to_col)),
        ) else {
            continue;
        };
        let is_main = main_edges.contains_key(&key);
        let span = main_edges
            .get(&key)
            .copied()
            .unwrap_or_else(|| dominant_prepend(&stats.prepend_weights));
        edges.push(GraphEdge {
            from_asn: key.from_asn,
            from_col: key.from_col,
            from_row,
            to_asn: key.to_asn,
            to_col: key.to_col,
            to_row,
            weight: stats.weight,
            span,
            is_main,
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

fn dominant_prepend(weights: &BTreeMap<usize, u32>) -> usize {
    weights
        .iter()
        .max_by(|(mult_a, w_a), (mult_b, w_b)| w_a.cmp(w_b).then_with(|| mult_a.cmp(mult_b)))
        .map_or(1, |(&mult, _)| mult.max(1))
}

fn refine_column_order(
    col: usize,
    adj: &CellNeighborMap,
    ord: &BTreeMap<u32, f64>,
    max_count: usize,
    col_members: &mut BTreeMap<usize, Vec<u32>>,
    cell_pos: &mut BTreeMap<(u32, usize), f64>,
) {
    let Some(members) = col_members.get(&col).cloned() else {
        return;
    };
    let mut scored: Vec<(u32, f64)> = Vec::with_capacity(members.len());
    for asn in members {
        let cur = cell_pos.get(&(asn, col)).copied().unwrap_or(0.0);
        let next = if let Some(neighbors) = adj.get(&(asn, col))
            && !neighbors.is_empty()
        {
            let mut weighted_sum = 0.0;
            let mut total_w = 0.0;
            for &(nb_cell, w) in neighbors {
                if let Some(&nb_p) = cell_pos.get(&nb_cell) {
                    let wf = f64::from(w.max(1));
                    weighted_sum += wf * nb_p;
                    total_w += wf;
                }
            }
            if total_w > 0.0 {
                0.75 * (weighted_sum / total_w) + 0.25 * cur
            } else {
                cur
            }
        } else {
            cur
        };
        scored.push((asn, next));
    }
    scored.sort_by(|(a, sa), (b, sb)| {
        sa.total_cmp(sb)
            .then_with(|| ord[a].total_cmp(&ord[b]))
            .then_with(|| a.cmp(b))
    });
    let offset = (max_count.saturating_sub(scored.len())) as f64 / 2.0;
    let sorted_asns: Vec<u32> = scored.into_iter().map(|(asn, _)| asn).collect();
    for (idx, &asn) in sorted_asns.iter().enumerate() {
        cell_pos.insert((asn, col), offset + idx as f64);
    }
    col_members.insert(col, sorted_asns);
}

fn assign_column_rows(
    col: usize,
    max_count: usize,
    col_members: &BTreeMap<usize, Vec<u32>>,
    in_adj: &CellNeighborMap,
    out_adj: &CellNeighborMap,
    cell_row: &mut BTreeMap<(u32, usize), usize>,
) {
    let Some(members) = col_members.get(&col) else {
        return;
    };
    if members.is_empty() {
        return;
    }
    let k = members.len();
    let default_start = max_count.saturating_sub(k) / 2;
    let mut ideals = Vec::with_capacity(k);

    for (idx, &asn) in members.iter().enumerate() {
        let mut weighted_sum = 0.0;
        let mut total_w = 0.0;
        let mut nb_weights: BTreeMap<usize, u32> = BTreeMap::new();
        for adj in [in_adj, out_adj] {
            if let Some(neighbors) = adj.get(&(asn, col)) {
                for &(nb_cell, w) in neighbors {
                    if let Some(&nb_row) = cell_row.get(&nb_cell) {
                        let w1 = w.max(1);
                        weighted_sum += f64::from(w1) * (nb_row as f64);
                        total_w += f64::from(w1);
                        let entry = nb_weights.entry(nb_row).or_insert(0);
                        *entry = entry.saturating_add(w1);
                    }
                }
            }
        }
        let ideal = if total_w > 0.0 {
            let avg = (weighted_sum / total_w).round() as usize;
            if nb_weights.contains_key(&avg) {
                avg
            } else {
                // Snap to the closest actual neighbor row (tie-breaking on higher weight) so boxes
                // align directly with a connected peer instead of pointing into an empty row gap.
                nb_weights
                    .iter()
                    .max_by(|(r_a, w_a), (r_b, w_b)| {
                        r_b.abs_diff(avg)
                            .cmp(&r_a.abs_diff(avg))
                            .then_with(|| w_a.cmp(w_b))
                            .then_with(|| r_b.cmp(r_a))
                    })
                    .map_or(avg, |(&r, _)| r)
            }
        } else {
            default_start + idx
        };
        ideals.push(ideal);
    }

    // Project `ideals` onto strictly increasing rows within `0..max_count`.
    let mut rows = Vec::with_capacity(k);
    for (idx, &ideal) in ideals.iter().enumerate() {
        let min_allowed = if idx == 0 { 0 } else { rows[idx - 1] + 1 };
        rows.push(ideal.max(min_allowed));
    }
    let max_target = max_count.saturating_sub(1);
    if k <= max_count && rows[k - 1] > max_target {
        rows[k - 1] = max_target;
        for idx in (0..(k - 1)).rev() {
            rows[idx] = rows[idx].min(rows[idx + 1].saturating_sub(1));
        }
    }

    for (&asn, row) in members.iter().zip(rows) {
        cell_row.insert((asn, col), row);
    }
}

fn collapse_prepends(path: &BgpPath, use_tier1: bool) -> Vec<HopCell> {
    let source = if path.asns_raw.is_empty() {
        &path.asns
    } else {
        &path.asns_raw
    };
    if use_tier1 {
        let trimmed = asns::trunc_to_tier1(source);
        let mut seq: Vec<HopCell> = Vec::with_capacity(trimmed.len());
        for &asn in trimmed.iter().rev() {
            if let Some(last) = seq.last_mut()
                && last.asn == asn
            {
                last.repeat += 1;
                continue;
            }
            seq.push(HopCell { asn, repeat: 1 });
        }
        seq
    } else {
        let mut clean_fwd: Vec<(u32, usize)> = Vec::with_capacity(source.len());
        for &asn in source {
            if let Some(last) = clean_fwd.last_mut()
                && last.0 == asn
            {
                last.1 += 1;
                continue;
            }
            clean_fwd.push((asn, 1));
        }
        let without_collector = if clean_fwd.len() > 1 {
            &clean_fwd[1..]
        } else {
            &clean_fwd[..]
        };
        without_collector
            .iter()
            .rev()
            .take(FALLBACK_MAX_HOPS)
            .map(|&(asn, repeat)| HopCell { asn, repeat })
            .collect()
    }
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
             fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{sw:.2}\" \
             opacity=\"{opacity}\"{dash}/>"
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
// 3. Two-port terminal Unicode 2D box-and-wire renderer (`--format text`)
// ---------------------------------------------------------------------------

const TERM_ROW_H: usize = 4;

/// Display columns taken by a right-hand band continuation stub (`══▸` or `──▸`).
const CONT_STUB_W: usize = 3;

#[derive(Clone, Copy)]
struct BoxPreset {
    inner_w: usize,
    min_gap: usize,
}

const STANDARD_BOX: BoxPreset = BoxPreset {
    inner_w: 14,
    min_gap: 6,
};

const COMPACT_BOX: BoxPreset = BoxPreset {
    inner_w: 11,
    min_gap: 5,
};

const BIT_LEFT: u8 = 1;
const BIT_RIGHT: u8 = 2;
const BIT_UP: u8 = 4;
const BIT_DOWN: u8 = 8;
const BIT_HEAVY: u8 = 16;
const BIT_DASH: u8 = 32;

struct GridMetrics {
    preset: BoxPreset,
    box_w: usize,
    col_x: Vec<usize>,
    gap_x: Vec<usize>,
    total_w: usize,
}

/// Renders `graph` as Unicode rounded boxes connected by two-port orthogonal wires (`═▸` main
/// path, `─▸` secondary paths, `┄×N▸` collapsed prepends):
/// - outgoing wires exit on box row 1 (`y0 + 1`) while cross-row incoming wires enter on box row 2
///   (`y0 + 2`), preventing false T-junction welds between unrelated rows;
/// - shrinks from 16-column boxes to 13-column compact boxes when a single band fits `max_width`;
/// - wraps deep multi-layer topologies (`5+` layers on 80-col terminals) into horizontal
///   continuation bands (`{col} (cont.)`) so 2D boxes remain intact across arbitrary hop depths;
/// - falls back to an indented Unicode tree only on ultra-narrow (`< 42`-column) terminals.
#[must_use]
pub fn render_terminal(
    graph: &PositionedGraph,
    indent: &str,
    max_width: Option<u16>,
) -> Vec<String> {
    let labels = default_col_labels(graph.max_col);
    let empty_cont = BTreeMap::new();
    let indent_w = indent.width();

    let (gap_tracks, gap_badge_w) = gap_routing_info(graph);
    let std_metrics = compute_grid_metrics(graph.max_col, &gap_tracks, &gap_badge_w, STANDARD_BOX);

    match max_width {
        None => {
            if std_metrics.total_w <= UNBOUNDED_BAND_BUDGET || graph.max_col <= 1 {
                render_band(
                    graph,
                    &labels,
                    &empty_cont,
                    &gap_tracks,
                    &std_metrics,
                    indent,
                )
            } else {
                render_banded(graph, indent, UNBOUNDED_WRAP_TARGET)
                    .unwrap_or_else(|| render_tree_fallback(graph, indent))
            }
        }
        Some(limit) => {
            let limit_usize = usize::from(limit);
            if graph.max_col == 0 || indent_w + std_metrics.total_w <= limit_usize {
                return render_band(
                    graph,
                    &labels,
                    &empty_cont,
                    &gap_tracks,
                    &std_metrics,
                    indent,
                );
            }
            let compact =
                compute_grid_metrics(graph.max_col, &gap_tracks, &gap_badge_w, COMPACT_BOX);
            if indent_w + compact.total_w <= limit_usize {
                return render_band(graph, &labels, &empty_cont, &gap_tracks, &compact, indent);
            }
            let budget = limit_usize.saturating_sub(indent_w);
            render_banded(graph, indent, budget)
                .unwrap_or_else(|| render_tree_fallback(graph, indent))
        }
    }
}

fn default_col_labels(max_col: usize) -> Vec<String> {
    (0..=max_col)
        .map(|c| {
            if c == 0 {
                "origin".to_owned()
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn gap_routing_info(graph: &PositionedGraph) -> (Vec<Vec<usize>>, Vec<usize>) {
    let mut gap_tracks: Vec<Vec<usize>> = Vec::with_capacity(graph.max_col);
    let mut gap_badge_w: Vec<usize> = Vec::with_capacity(graph.max_col);
    for g in 0..graph.max_col {
        let mut sources = BTreeSet::new();
        let mut max_bw = 0_usize;
        for e in &graph.edges {
            if e.to_col == g + 1 {
                if e.from_row != e.to_row {
                    sources.insert(e.from_row);
                }
                if e.span >= 2 {
                    max_bw = max_bw.max(format!("×{}", e.span).width());
                }
            }
        }
        gap_tracks.push(sources.into_iter().collect());
        gap_badge_w.push(max_bw);
    }
    (gap_tracks, gap_badge_w)
}

/// Splits a wide multi-layer `graph` into horizontal continuation bands (`[s..=e]`, advancing
/// `s = e`), returning `None` only when `budget` cannot fit even two compact columns side-by-side.
fn render_banded(graph: &PositionedGraph, indent: &str, budget: usize) -> Option<Vec<String>> {
    // Prefer full 16-column boxes inside bands whenever at least 3 columns fit cleanly (e.g. 80+
    // column terminals); otherwise use 13-column compact boxes down to 2-column narrow panes.
    let min_three_std = 3 * (STANDARD_BOX.inner_w + 2) + 2 * STANDARD_BOX.min_gap + CONT_STUB_W;
    let preset = if budget >= min_three_std {
        STANDARD_BOX
    } else {
        COMPACT_BOX
    };

    let mut out_lines: Vec<String> = Vec::new();
    let mut start_col = 0_usize;

    while start_col < graph.max_col {
        let mut chosen = None;
        for cand_end in ((start_col + 1)..=graph.max_col).rev() {
            let (sub_graph, sub_labels, right_cont) = extract_band(graph, start_col, cand_end);
            let (tracks, badge_w) = gap_routing_info(&sub_graph);
            let metrics = compute_grid_metrics(sub_graph.max_col, &tracks, &badge_w, preset);
            let stub_w = if right_cont.is_empty() {
                0
            } else {
                CONT_STUB_W
            };
            if metrics.total_w + stub_w <= budget {
                chosen = Some((cand_end, sub_graph, sub_labels, right_cont, tracks, metrics));
                break;
            }
        }

        // If `STANDARD_BOX` failed because a single gap had unusually wide fan-out tracks, retry
        // this band with `COMPACT_BOX` before giving up to tree fallback.
        if chosen.is_none() && preset.inner_w == STANDARD_BOX.inner_w {
            for cand_end in ((start_col + 1)..=graph.max_col).rev() {
                let (sub_graph, sub_labels, right_cont) = extract_band(graph, start_col, cand_end);
                let (tracks, badge_w) = gap_routing_info(&sub_graph);
                let metrics =
                    compute_grid_metrics(sub_graph.max_col, &tracks, &badge_w, COMPACT_BOX);
                let stub_w = if right_cont.is_empty() {
                    0
                } else {
                    CONT_STUB_W
                };
                if metrics.total_w + stub_w <= budget {
                    chosen = Some((cand_end, sub_graph, sub_labels, right_cont, tracks, metrics));
                    break;
                }
            }
        }

        let (end_col, sub_graph, sub_labels, right_cont, tracks, metrics) = chosen?;
        if !out_lines.is_empty() {
            out_lines.push(String::new());
        }
        let band_lines = render_band(
            &sub_graph,
            &sub_labels,
            &right_cont,
            &tracks,
            &metrics,
            indent,
        );
        out_lines.extend(band_lines);
        start_col = end_col;
    }

    Some(out_lines)
}

fn extract_band(
    graph: &PositionedGraph,
    start_col: usize,
    end_col: usize,
) -> (PositionedGraph, Vec<String>, BTreeMap<usize, bool>) {
    let active_edges: Vec<&GraphEdge> = graph
        .edges
        .iter()
        .filter(|e| e.from_col >= start_col && e.to_col <= end_col)
        .collect();

    let mut active_cells: BTreeSet<(u32, usize)> = BTreeSet::new();
    if start_col == 0 {
        for b in &graph.boxes {
            if b.col == 0 {
                active_cells.insert((b.asn, b.col));
            }
        }
    }
    for e in &active_edges {
        active_cells.insert((e.from_asn, e.from_col));
        active_cells.insert((e.to_asn, e.to_col));
    }

    let mut used_rows_set = BTreeSet::new();
    for b in &graph.boxes {
        if active_cells.contains(&(b.asn, b.col)) {
            used_rows_set.insert(b.row);
        }
    }
    let used_rows: Vec<usize> = used_rows_set.into_iter().collect();
    let remap_row = |orig_row: usize| used_rows.binary_search(&orig_row).unwrap_or(0);

    let mut boxes = Vec::new();
    for b in &graph.boxes {
        if active_cells.contains(&(b.asn, b.col)) {
            boxes.push(GraphBox {
                asn: b.asn,
                col: b.col - start_col,
                row: remap_row(b.row),
                is_origin: b.is_origin,
                is_tier1: b.is_tier1,
                name: b.name.clone(),
            });
        }
    }
    let edges: Vec<GraphEdge> = active_edges
        .into_iter()
        .map(|e| GraphEdge {
            from_asn: e.from_asn,
            from_col: e.from_col - start_col,
            from_row: remap_row(e.from_row),
            to_asn: e.to_asn,
            to_col: e.to_col - start_col,
            to_row: remap_row(e.to_row),
            weight: e.weight,
            span: e.span,
            is_main: e.is_main,
        })
        .collect();

    let mut right_cont = BTreeMap::new();
    if end_col < graph.max_col {
        for e in &graph.edges {
            if e.from_col == end_col && active_cells.contains(&(e.from_asn, end_col)) {
                let entry = right_cont.entry(remap_row(e.from_row)).or_insert(false);
                *entry |= e.is_main;
            }
        }
    }

    let labels: Vec<String> = (start_col..=end_col)
        .map(|c| {
            if c == 0 {
                "origin".to_owned()
            } else if c == start_col {
                format!("{c} (cont.)")
            } else {
                c.to_string()
            }
        })
        .collect();

    let sub_graph = PositionedGraph {
        max_col: end_col - start_col,
        max_row: used_rows.len().saturating_sub(1),
        boxes,
        edges,
    };
    (sub_graph, labels, right_cont)
}

fn render_band(
    graph: &PositionedGraph,
    col_labels: &[String],
    right_cont: &BTreeMap<usize, bool>,
    gap_tracks: &[Vec<usize>],
    metrics: &GridMetrics,
    indent: &str,
) -> Vec<String> {
    let box_w = metrics.box_w;
    let inner_w = metrics.preset.inner_w;
    let stub_w = if right_cont.is_empty() {
        0
    } else {
        CONT_STUB_W
    };
    let total_w = metrics.total_w + stub_w;
    let col_x = &metrics.col_x;
    let gap_x = &metrics.gap_x;

    let total_h = 1 + (graph.max_row + 1) * TERM_ROW_H;
    let mut chars: Vec<Vec<char>> = vec![vec![' '; total_w]; total_h];
    // `wide_cont[y][x]` marks the trailing column of a 2-column-wide glyph so serialization
    // emits the glyph once while keeping exact display-column coordinates across the grid.
    let mut wide_cont: Vec<Vec<bool>> = vec![vec![false; total_w]; total_h];
    let mut wires: Vec<Vec<u8>> = vec![vec![0; total_w]; total_h];

    // 1. Column distance header row (`y = 0`).
    for (col_idx, &cx_val) in col_x.iter().enumerate() {
        let label = col_labels.get(col_idx).map_or("", String::as_str);
        let pad_left = box_w.saturating_sub(label.width()) / 2;
        put_str(&mut chars[0], &mut wide_cont[0], cx_val + pad_left, label);
    }

    // 2. Two-port orthogonal wire bitmasks across inter-column gaps:
    //    - `y_out(row)` = line 1 inside box (`│ ASxxxxx ★ │`), used by outgoing & straight wires;
    //    - `y_in(row)`  = line 2 inside box (`│ Name      │`), used by cross-row incoming wires.
    let y_out = |row: usize| 2 + row * TERM_ROW_H;
    let y_in = |row: usize| 3 + row * TERM_ROW_H;

    for e in &graph.edges {
        let x_start = col_x[e.from_col] + box_w;
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

        if e.from_row == e.to_row {
            let y = y_out(e.from_row);
            for cell in &mut wires[y][x_start..x_end] {
                *cell |= BIT_LEFT | BIT_RIGHT | style;
            }
        } else {
            let y1 = y_out(e.from_row);
            let y2 = y_in(e.to_row);
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
        for x in 0..metrics.total_w {
            let mask = wires[y][x];
            if mask != 0 {
                chars[y][x] = wire_glyph(mask);
            }
        }
    }

    // Place arrowheads and destination-side prepend `×N` badges strictly to the right of all
    // vertical tracks so badges never overwrite wire junctions.
    for e in &graph.edges {
        let x_end = col_x[e.to_col].saturating_sub(1);
        let y_dest = if e.from_row == e.to_row {
            y_out(e.to_row)
        } else {
            y_in(e.to_row)
        };
        chars[y_dest][x_end] = '▸';
        if e.span >= 2 {
            let badge = format!("×{}", e.span);
            let bw = badge.width();
            let g = e.to_col - 1;
            let min_bx = gap_x[g] + 1 + gap_tracks[g].len();
            if x_end >= min_bx + bw {
                let bx = x_end - bw;
                put_str(&mut chars[y_dest], &mut wide_cont[y_dest], bx, &badge);
            }
        }
    }

    // Draw right-edge continuation stubs (`══▸` / `──▸`) for nodes continuing into the next band.
    for (&local_row, &is_main) in right_cont {
        let stub = if is_main { "══▸" } else { "──▸" };
        let y = y_out(local_row);
        put_str(&mut chars[y], &mut wide_cont[y], metrics.total_w, stub);
    }

    // 3. Draw rounded ASN node boxes (`╭─╮│╰─╯`).
    let top_border = format!("╭{}╮", "─".repeat(inner_w));
    let bot_border = format!("╰{}╯", "─".repeat(inner_w));
    for b in &graph.boxes {
        let x = col_x[b.col];
        let y0 = 1 + b.row * TERM_ROW_H;
        let star = if b.is_tier1 { " ★" } else { "" };
        let line1 = fit_cell(&format!(" AS{}{star}", b.asn), inner_w);
        let line2 = fit_cell(&format!(" {}", b.name), inner_w);

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

fn compute_grid_metrics(
    max_col: usize,
    gap_tracks: &[Vec<usize>],
    gap_badge_w: &[usize],
    preset: BoxPreset,
) -> GridMetrics {
    let box_w = preset.inner_w + 2;
    let mut col_x = vec![0_usize; max_col + 1];
    let mut gap_x = vec![0_usize; max_col];
    let mut cur_x = 0_usize;
    for c in 0..=max_col {
        col_x[c] = cur_x;
        cur_x += box_w;
        if c < max_col {
            gap_x[c] = cur_x;
            let tracks = gap_tracks[c].len();
            let bw = gap_badge_w[c];
            let needed = if bw > 0 { tracks + bw + 3 } else { tracks + 4 };
            cur_x += needed.max(preset.min_gap);
        }
    }
    GridMetrics {
        preset,
        box_w,
        col_x,
        gap_x,
        total_w: cur_x,
    }
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

/// Compact Unicode tree fallback when the terminal window cannot fit even two columns side-by-side.
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
        let has_further = children.get(&target_key).is_some_and(|c| !c.is_empty());
        let first_visit = visited.insert(target_key);
        let shared_note = if !first_visit && has_further {
            " (↑)"
        } else {
            ""
        };
        lines.push(format!(
            "{prefix}{branch}{wire} {label}{span_note}{shared_note}"
        ));
        if first_visit {
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
