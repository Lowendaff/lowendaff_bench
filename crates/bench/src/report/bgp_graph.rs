//! BGP `AS_PATH` topology layout and vector renderers:
//! - Sugiyama clean-hop layered layout with Tier-1-deduplicated path selection, centered-trunk
//!   DFS, barycenter crossing reduction, and neighbor-snapped monotonic row assignment;
//! - self-contained inline-styled SVG renderer (uploaded to `imageupload.app` in `--format text`
//!   and embedded directly in `--format json`);
//! - GitHub-Flavored Markdown `mermaid` `flowchart LR` renderer (for `--format markdown`).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::collections::btree_map::Entry;
use std::fmt::Write;

use plugin_network::bgp::BgpPath;
use plugin_network::bgp::FamilyBgp;
use plugin_network::bgp::asns;

/// Maximum distinct Tier-1-collapsed paths included in Markdown `mermaid` diagrams so wide
/// anycast networks stay readable inline. SVG keeps every distinct collapsed path.
pub const COMPACT_MAX_PATHS: usize = 14;

/// Maximum clean hops kept when a prefix has no Tier-1 transit path at all (e.g. pure IXP peering).
const FALLBACK_MAX_HOPS: usize = 5;

/// Number of alternating left-to-right / right-to-left barycenter sweeps for crossing reduction.
const BARYCENTER_SWEEPS: usize = 3;

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

/// Positioned layered graph shared by the SVG and Mermaid renderers.
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

    let assigned = members.iter().zip(rows);
    for (&asn, row) in assigned {
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
        let sum: f64 = values.iter().sum();
        sum / (values.len() as f64)
    };
    ord.insert(asn, mean);
    mean
}

// ---------------------------------------------------------------------------
// 1. Self-contained inline-styled SVG renderer (`--format text` upload & `--format json`)
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

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}
