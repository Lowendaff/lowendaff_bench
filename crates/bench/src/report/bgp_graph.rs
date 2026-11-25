//! BGP `AS_PATH` topology layout and vector renderers:
//! - Sugiyama layered layout aligned with `peer.as` (collector-nearest Tier-1 truncation with
//!   uncovered direct-upstream supplementation, raw BGP hop columns capped per prepend span,
//!   peer-weighted centered-trunk DFS, dummy-waypoint routing around intermediate boxes, and
//!   Pool-Adjacent-Violators continuous vertical centering);
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

/// Maximum distinct collapsed paths included in Markdown `mermaid` diagrams so wide anycast
/// networks stay readable inline. SVG keeps every distinct collapsed path.
pub const COMPACT_MAX_PATHS: usize = 16;

/// Maximum column jump added by a single prepended hop (`×2`..=`×4` advance 2..=4 columns matching
/// `peer.as`; extreme `×5`..=`×9` prepends cap at 4 columns while preserving their exact `×N`
/// badge).
pub const MAX_PREPEND_SPAN: usize = 4;

/// Maximum clean hops kept when a path contains no Tier-1 transit hop.
const FALLBACK_MAX_HOPS: usize = 5;

/// Number of alternating right-to-left / left-to-right barycenter sweeps for crossing reduction.
const BARYCENTER_SWEEPS: usize = 6;

/// Minimum vertical slot separation between two real ASN boxes in the same column (`48 px`).
const GAP_BOX_BOX: f64 = 1.0;

/// Minimum vertical slot separation between a real ASN box and a pass-through wire waypoint.
const GAP_BOX_DUMMY: f64 = 0.68;

/// Minimum vertical slot separation between two parallel pass-through wire waypoints.
const GAP_DUMMY_DUMMY: f64 = 0.45;

/// Display-width budget (ASCII = 1, CJK = 2) for carrier subtitles inside `120 px` diagram boxes.
const BOX_SUBTITLE_WIDTH: usize = 18;

/// One positioned `(asn, column)` node box in [`PositionedGraph`].
#[derive(Debug, Clone, PartialEq)]
pub struct GraphBox {
    /// Autonomous System number.
    pub asn: u32,
    /// BGP hop distance column from origin (`0` = origin on the left, increasing rightward).
    pub col: usize,
    /// Strictly increasing 0-based vertical rank within `col`.
    pub row: usize,
    /// Continuous vertical slot position (`>= 0.0`, separated by `>= 1.0` within `col`).
    pub y_slot: f64,
    /// Whether `asn` is an origin of the prefix.
    pub is_origin: bool,
    /// Whether `asn` is a global Tier-1 backbone (`★`).
    pub is_tier1: bool,
    /// Concise display name, or empty when unknown.
    pub name: String,
}

/// One directed link from `from` (closer to origin, left) to `to` (closer to Tier-1, right).
#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    /// Downstream ASN (closer to origin).
    pub from_asn: u32,
    /// Column of `from_asn`.
    pub from_col: usize,
    /// Vertical rank of `from_asn` within `from_col`.
    pub from_row: usize,
    /// Continuous vertical slot of `from_asn`.
    pub from_slot: f64,
    /// Upstream ASN (closer to Tier-1).
    pub to_asn: u32,
    /// Column of `to_asn` (`> from_col`).
    pub to_col: usize,
    /// Vertical rank of `to_asn` within `to_col`.
    pub to_row: usize,
    /// Continuous vertical slot of `to_asn`.
    pub to_slot: f64,
    /// Unobstructed intermediate `(col, y_slot)` waypoints when `to_col > from_col + 1`.
    pub waypoints: Vec<(usize, f64)>,
    /// Aggregated RIS peer count across paths traversing this edge.
    pub weight: u32,
    /// Dominant `AS_PATH` prepend multiplicity on `from_asn` (`1` = normal, `>= 2` = `×N` prepend).
    pub span: usize,
    /// Whether this edge belongs to the highest-peer main path.
    pub is_main: bool,
}

/// Positioned layered graph shared by the SVG and Mermaid renderers.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionedGraph {
    /// Rightmost column index (`0` when only the origin box exists).
    pub max_col: usize,
    /// Highest 0-based intra-column rank across all columns.
    pub max_row: usize,
    /// Maximum vertical `y_slot` across all positioned boxes and wire waypoints.
    pub max_slot: f64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LayerNode {
    Box { asn: u32, col: usize },
    Dummy { edge_idx: usize, col: usize },
}

impl LayerNode {
    const fn is_box(self) -> bool {
        matches!(self, Self::Box { .. })
    }
}

#[derive(Debug, Clone, Copy)]
struct NeighborLink {
    node: LayerNode,
    weight: u32,
    is_main: bool,
}

type LayerAdjMap = BTreeMap<LayerNode, Vec<NeighborLink>>;

/// Computes the Sugiyama BGP path layout for `family`, or `None` when `family` is not announced or
/// has no paths.
#[must_use]
pub fn layout(family: &FamilyBgp, max_paths: Option<usize>) -> Option<PositionedGraph> {
    if !family.announced || family.paths.is_empty() {
        return None;
    }

    let mut origin_set = BTreeSet::new();
    for o in &family.origins {
        origin_set.insert(o.asn);
    }
    if origin_set.is_empty()
        && let Some(origin) = family.origin_asn
    {
        origin_set.insert(origin);
    }

    let candidates = select_candidate_paths(family, &origin_set);
    if candidates.is_empty() {
        return None;
    }

    // Deduplicate and sum peer counts by collapsed truncated ASN sequence before applying
    // `max_paths` so collector-side duplicates behind a single backbone never crowd out backups.
    let mut grouped: BTreeMap<Vec<u32>, CollapsedGroup> = BTreeMap::new();
    for path in candidates {
        let seq = collapse_truncated_hops(path);
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
        let hop_pairs = seq.iter().copied().zip(entry.hop_repeats.iter_mut());
        for (hop, rep_map) in hop_pairs {
            let rw = rep_map.entry(hop.repeat).or_insert(0);
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
    if origin_set.is_empty() {
        origin_set.insert(groups[0].asns[0]);
    }

    let mut cell_cols: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    let mut edge_stats: BTreeMap<EdgeKey, EdgeStats> = BTreeMap::new();
    let mut main_edges: BTreeMap<EdgeKey, usize> = BTreeMap::new();

    let indexed_groups = groups.iter().enumerate();
    for (grp_idx, grp) in indexed_groups {
        let mut cols = Vec::with_capacity(grp.asns.len());
        let mut preps = Vec::with_capacity(grp.asns.len().saturating_sub(1));
        let mut cur_col = 0_usize;
        cols.push(cur_col);
        let hop_slice = &grp.hop_repeats[..grp.asns.len().saturating_sub(1)];
        for rep_weights in hop_slice {
            let rep = dominant_prepend(rep_weights);
            preps.push(rep);
            cur_col = cur_col.saturating_add(rep.clamp(1, MAX_PREPEND_SPAN));
            cols.push(cur_col);
        }
        let asn_cols = grp.asns.iter().copied().zip(cols.iter().copied());
        for (asn, col) in asn_cols {
            cell_cols.entry(asn).or_default().insert(col);
        }
        let edge_steps = grp
            .asns
            .windows(2)
            .zip(cols.windows(2))
            .zip(preps.iter().copied());
        for ((asn_pair, col_pair), rep) in edge_steps {
            let key = EdgeKey {
                from_asn: asn_pair[0],
                from_col: col_pair[0],
                to_asn: asn_pair[1],
                to_col: col_pair[1],
            };
            let stats = edge_stats.entry(key).or_default();
            stats.weight = stats.weight.saturating_add(grp.peers);
            let pw = stats.prepend_weights.entry(rep).or_insert(0);
            *pw = pw.saturating_add(grp.peers);
            if grp_idx == 0 {
                main_edges.insert(key, rep);
            }
        }
    }

    if cell_cols.is_empty() {
        return None;
    }

    let all_asns: Vec<u32> = cell_cols.keys().copied().collect();
    let mut max_col = 0;
    for cols in cell_cols.values() {
        if let Some(&c) = cols.iter().next_back() {
            max_col = max_col.max(c);
        }
    }

    // Pass 1: Peer-weighted subtree sizing and centered-trunk DFS from origin(s).
    let mut children_of: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut pair_weight: BTreeMap<(u32, u32), u64> = BTreeMap::new();
    for (key, stats) in &edge_stats {
        children_of
            .entry(key.from_asn)
            .or_default()
            .insert(key.to_asn);
        let pw = pair_weight.entry((key.from_asn, key.to_asn)).or_insert(0);
        *pw = pw.saturating_add(u64::from(stats.weight));
    }

    let mut subtree_leaves: BTreeMap<u32, usize> = BTreeMap::new();
    let mut subtree_weight: BTreeMap<u32, u64> = BTreeMap::new();
    for &asn in &all_asns {
        compute_tree_metrics(
            asn,
            &children_of,
            &pair_weight,
            &mut subtree_leaves,
            &mut subtree_weight,
        );
    }

    let mut roots: Vec<u32> = Vec::new();
    for &asn in &all_asns {
        if origin_set.contains(&asn) {
            roots.push(asn);
        }
    }
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
            &pair_weight,
            &subtree_leaves,
            &subtree_weight,
            &mut seen,
            &mut ord,
            &mut leaf_counter,
        );
    }
    for &asn in &all_asns {
        if let Entry::Vacant(vacant) = ord.entry(asn) {
            vacant.insert(usize_to_f64(leaf_counter));
            leaf_counter += 1;
        }
    }

    // Pass 2: Sugiyama layer graph with virtual dummy waypoints on multi-column edges so long
    // prepend curves route through clear horizontal channels instead of crossing behind boxes.
    let edge_keys: Vec<EdgeKey> = edge_stats.keys().copied().collect();
    let mut col_items: BTreeMap<usize, Vec<LayerNode>> = BTreeMap::new();
    let mut item_ord: BTreeMap<LayerNode, (f64, u8, usize)> = BTreeMap::new();

    for &asn in &all_asns {
        for &col in &cell_cols[&asn] {
            let node = LayerNode::Box { asn, col };
            col_items.entry(col).or_default().push(node);
            item_ord.insert(node, (ord[&asn], 0, asn as usize));
        }
    }

    let mut in_adj: LayerAdjMap = BTreeMap::new();
    let mut out_adj: LayerAdjMap = BTreeMap::new();
    let mut edge_chains: Vec<Vec<LayerNode>> = Vec::with_capacity(edge_keys.len());

    let indexed_edges = edge_keys.iter().copied().enumerate();
    for (edge_idx, key) in indexed_edges {
        let weight = edge_stats[&key].weight;
        let is_main = main_edges.contains_key(&key);
        let u_node = LayerNode::Box {
            asn: key.from_asn,
            col: key.from_col,
        };
        let v_node = LayerNode::Box {
            asn: key.to_asn,
            col: key.to_col,
        };
        let mut chain = Vec::with_capacity(key.to_col.saturating_sub(key.from_col) + 1);
        chain.push(u_node);
        let dummy_ord = (ord[&key.to_asn], 1, edge_idx);
        for mid_col in (key.from_col + 1)..key.to_col {
            let dm = LayerNode::Dummy {
                edge_idx,
                col: mid_col,
            };
            col_items.entry(mid_col).or_default().push(dm);
            item_ord.insert(dm, dummy_ord);
            chain.push(dm);
        }
        chain.push(v_node);
        let chain_pairs = chain.windows(2);
        for pair in chain_pairs {
            let src = pair[0];
            let dst = pair[1];
            out_adj.entry(src).or_default().push(NeighborLink {
                node: dst,
                weight,
                is_main,
            });
            in_adj.entry(dst).or_default().push(NeighborLink {
                node: src,
                weight,
                is_main,
            });
        }
        edge_chains.push(chain);
    }

    for items in col_items.values_mut() {
        items.sort_by(|a, b| cmp_item_ord(item_ord[a], item_ord[b]));
    }

    let mut max_span = 0.0_f64;
    for items in col_items.values() {
        max_span = max_span.max(column_span(items));
    }

    let mut item_pos: BTreeMap<LayerNode, f64> = BTreeMap::new();
    for items in col_items.values() {
        seed_column_positions(items, max_span, &mut item_pos);
    }

    for sweep in 0..BARYCENTER_SWEEPS {
        if sweep % 2 == 0 {
            let mut col = max_col;
            while col > 0 {
                col -= 1;
                refine_column_order(
                    col,
                    false,
                    &in_adj,
                    &out_adj,
                    &item_ord,
                    max_span,
                    &mut col_items,
                    &mut item_pos,
                );
            }
        } else {
            for col in 1..=max_col {
                refine_column_order(
                    col,
                    true,
                    &in_adj,
                    &out_adj,
                    &item_ord,
                    max_span,
                    &mut col_items,
                    &mut item_pos,
                );
            }
        }
    }

    // Pass 3: Anchor the tallest column and assign continuous vertical slots via variable-gap
    // Pool-Adjacent-Violators isotonic regression, followed by two smoothing passes.
    let anchor_col = (0..=max_col)
        .max_by(|&a, &b| {
            let span_a = col_items.get(&a).map_or(0.0, |v| column_span(v));
            let span_b = col_items.get(&b).map_or(0.0, |v| column_span(v));
            span_a
                .total_cmp(&span_b)
                .then_with(|| {
                    let len_a = col_items.get(&a).map_or(0, Vec::len);
                    let len_b = col_items.get(&b).map_or(0, Vec::len);
                    len_a.cmp(&len_b)
                })
                .then_with(|| b.cmp(&a))
        })
        .unwrap_or(0);

    let mut item_slot: BTreeMap<LayerNode, f64> = BTreeMap::new();
    assign_column_slots(
        anchor_col,
        max_span,
        &col_items,
        &in_adj,
        &out_adj,
        &mut item_slot,
    );
    let mut left_col = anchor_col;
    while left_col > 0 {
        left_col -= 1;
        assign_column_slots(
            left_col,
            max_span,
            &col_items,
            &in_adj,
            &out_adj,
            &mut item_slot,
        );
    }
    for right_col in (anchor_col + 1)..=max_col {
        assign_column_slots(
            right_col,
            max_span,
            &col_items,
            &in_adj,
            &out_adj,
            &mut item_slot,
        );
    }
    for pass in 0..2 {
        if pass == 0 {
            let mut col = max_col + 1;
            while col > 0 {
                col -= 1;
                if col != anchor_col {
                    assign_column_slots(
                        col,
                        max_span,
                        &col_items,
                        &in_adj,
                        &out_adj,
                        &mut item_slot,
                    );
                }
            }
        } else {
            for col in 0..=max_col {
                if col != anchor_col {
                    assign_column_slots(
                        col,
                        max_span,
                        &col_items,
                        &in_adj,
                        &out_adj,
                        &mut item_slot,
                    );
                }
            }
        }
    }

    let min_slot = item_slot.values().copied().fold(f64::INFINITY, f64::min);
    if min_slot.is_finite() && min_slot > 0.0 {
        for slot in item_slot.values_mut() {
            *slot = (*slot - min_slot).max(0.0);
        }
    }
    let max_slot = item_slot.values().copied().fold(0.0_f64, f64::max);

    let mut box_rank: BTreeMap<(u32, usize), usize> = BTreeMap::new();
    let mut max_row = 0_usize;
    let mut boxes = Vec::new();

    for (&col, items) in &col_items {
        let mut row_idx = 0_usize;
        for &node in items {
            let LayerNode::Box { asn, .. } = node else {
                continue;
            };
            let y_slot = item_slot.get(&node).copied().unwrap_or(0.0);
            let is_origin = origin_set.contains(&asn);
            let is_tier1 = asns::is_tier1(asn);
            let name = family
                .asn_names
                .get(&asn)
                .cloned()
                .or_else(|| asns::known_name(asn).map(str::to_owned))
                .unwrap_or_default();
            box_rank.insert((asn, col), row_idx);
            max_row = max_row.max(row_idx);
            boxes.push(GraphBox {
                asn,
                col,
                row: row_idx,
                y_slot,
                is_origin,
                is_tier1,
                name,
            });
            row_idx += 1;
        }
    }
    boxes.sort_by_key(|b| (b.col, b.row, b.asn));

    let mut edges = Vec::with_capacity(edge_keys.len());
    let edge_with_chains = edge_keys.iter().copied().zip(edge_chains.iter());
    for (key, chain) in edge_with_chains {
        let stats = &edge_stats[&key];
        let (Some(&from_row), Some(&to_row)) = (
            box_rank.get(&(key.from_asn, key.from_col)),
            box_rank.get(&(key.to_asn, key.to_col)),
        ) else {
            continue;
        };
        let from_slot = item_slot.get(&chain[0]).copied().unwrap_or(0.0);
        let to_slot = item_slot
            .get(&chain[chain.len() - 1])
            .copied()
            .unwrap_or(0.0);
        let mut waypoints = Vec::with_capacity(chain.len().saturating_sub(2));
        for &mid_node in &chain[1..chain.len().saturating_sub(1)] {
            if let LayerNode::Dummy { col, .. } = mid_node
                && let Some(&slot) = item_slot.get(&mid_node)
            {
                waypoints.push((col, slot));
            }
        }
        let is_main = main_edges.contains_key(&key);
        let span = main_edges
            .get(&key)
            .copied()
            .unwrap_or_else(|| dominant_prepend(&stats.prepend_weights));
        edges.push(GraphEdge {
            from_asn: key.from_asn,
            from_col: key.from_col,
            from_row,
            from_slot,
            to_asn: key.to_asn,
            to_col: key.to_col,
            to_row,
            to_slot,
            waypoints,
            weight: stats.weight,
            span,
            is_main,
        });
    }
    // Draw main-path edges last so they sit above secondary edges without occlusion.
    edges.sort_by(|a, b| {
        a.is_main
            .cmp(&b.is_main)
            .then_with(|| a.from_col.cmp(&b.from_col))
            .then_with(|| a.from_slot.total_cmp(&b.from_slot))
            .then_with(|| a.to_slot.total_cmp(&b.to_slot))
    });

    Some(PositionedGraph {
        max_col,
        max_row,
        max_slot,
        boxes,
        edges,
    })
}

fn select_candidate_paths<'a>(
    family: &'a FamilyBgp,
    origin_set: &BTreeSet<u32>,
) -> Vec<&'a BgpPath> {
    let mut tier1_paths: Vec<&BgpPath> = Vec::new();
    let mut covered_upstreams: BTreeSet<u32> = BTreeSet::new();

    for path in &family.paths {
        if path.asns.is_empty() {
            continue;
        }
        let non_origin = &path.asns[..path.asns.len().saturating_sub(1)];
        let has_upstream_tier1 = non_origin.iter().any(|&a| asns::is_tier1(a));
        if has_upstream_tier1 {
            if path.asns.len() >= 2 {
                covered_upstreams.insert(path.asns[path.asns.len() - 2]);
            }
            tier1_paths.push(path);
        }
    }

    if tier1_paths.is_empty() {
        let mut fallback = Vec::new();
        for path in &family.paths {
            if !path.asns.is_empty() {
                fallback.push(path);
            }
        }
        return fallback;
    }

    // Supplement a representative path for any non-Tier-1 direct upstream carrying a meaningful
    // share (`>= 5%` of RIS peers, or `>= 2` peers on compact <=4-upstream prefixes) that has zero
    // Tier-1-transiting paths, keeping the diagram consistent with the upstream summary table.
    let total = u64::from(family.total_peers.max(1));
    let compact_upstreams = family.upstreams.len() <= 4;
    for up in &family.upstreams {
        if covered_upstreams.contains(&up.asn) || origin_set.contains(&up.asn) {
            continue;
        }
        let significant =
            u64::from(up.peers).saturating_mul(20) >= total || (compact_upstreams && up.peers >= 2);
        if !significant {
            continue;
        }
        if let Some(rep_path) = family
            .paths
            .iter()
            .filter(|p| p.asns.len() >= 2 && p.asns[p.asns.len() - 2] == up.asn)
            .max_by(|a, b| {
                a.peers
                    .cmp(&b.peers)
                    .then_with(|| a.is_best.cmp(&b.is_best))
                    .then_with(|| b.asns.len().cmp(&a.asns.len()))
            })
        {
            tier1_paths.push(rep_path);
            covered_upstreams.insert(up.asn);
        }
    }

    tier1_paths
}

fn collapse_truncated_hops(path: &BgpPath) -> Vec<HopCell> {
    let source = if path.asns_raw.is_empty() {
        &path.asns
    } else {
        &path.asns_raw
    };
    let truncated = asns::trunc_to_tier1(source);
    if truncated.is_empty() {
        return Vec::new();
    }
    let has_upstream_tier1 = truncated[..truncated.len().saturating_sub(1)]
        .iter()
        .any(|&a| asns::is_tier1(a));

    let mut rev_runs: Vec<HopCell> = Vec::with_capacity(truncated.len());
    let mut idx = truncated.len();
    while idx > 0 {
        idx -= 1;
        let asn = truncated[idx];
        if let Some(last) = rev_runs.last_mut()
            && last.asn == asn
        {
            last.repeat += 1;
            continue;
        }
        rev_runs.push(HopCell { asn, repeat: 1 });
    }
    if !has_upstream_tier1 && rev_runs.len() > FALLBACK_MAX_HOPS {
        rev_runs.truncate(FALLBACK_MAX_HOPS);
    }
    rev_runs
}

fn dominant_prepend(weights: &BTreeMap<usize, u32>) -> usize {
    weights
        .iter()
        .max_by(|(mult_a, w_a), (mult_b, w_b)| w_a.cmp(w_b).then_with(|| mult_a.cmp(mult_b)))
        .map_or(1, |(&mult, _)| mult.max(1))
}

fn pair_gap(prev: LayerNode, next: LayerNode) -> f64 {
    match (prev.is_box(), next.is_box()) {
        (true, true) => GAP_BOX_BOX,
        (false, false) => GAP_DUMMY_DUMMY,
        (true, false) | (false, true) => GAP_BOX_DUMMY,
    }
}

fn column_span(items: &[LayerNode]) -> f64 {
    if items.len() <= 1 {
        return 0.0;
    }
    let mut total = 0.0;
    let item_pairs = items.windows(2);
    for pair in item_pairs {
        total += pair_gap(pair[0], pair[1]);
    }
    total
}

fn seed_column_positions(
    items: &[LayerNode],
    max_span: f64,
    item_pos: &mut BTreeMap<LayerNode, f64>,
) {
    let span = column_span(items);
    let mut cur = (max_span - span).max(0.0) / 2.0;
    let mut prev: Option<LayerNode> = None;
    for &node in items {
        if let Some(prev_node) = prev {
            cur += pair_gap(prev_node, node);
        }
        item_pos.insert(node, cur);
        prev = Some(node);
    }
}

fn cmp_item_ord(a: (f64, u8, usize), b: (f64, u8, usize)) -> std::cmp::Ordering {
    a.0.total_cmp(&b.0)
        .then_with(|| a.1.cmp(&b.1))
        .then_with(|| a.2.cmp(&b.2))
}

#[allow(clippy::too_many_arguments)]
fn refine_column_order(
    col: usize,
    forward: bool,
    in_adj: &LayerAdjMap,
    out_adj: &LayerAdjMap,
    item_ord: &BTreeMap<LayerNode, (f64, u8, usize)>,
    max_span: f64,
    col_items: &mut BTreeMap<usize, Vec<LayerNode>>,
    item_pos: &mut BTreeMap<LayerNode, f64>,
) {
    let Some(items) = col_items.get(&col).cloned() else {
        return;
    };
    if items.is_empty() {
        return;
    }
    let mut scored: Vec<(LayerNode, f64)> = Vec::with_capacity(items.len());
    for node in items {
        let cur = item_pos.get(&node).copied().unwrap_or(0.0);
        let next = if node.is_box() {
            let primary = if forward { in_adj } else { out_adj };
            match neighbor_barycenter(node, primary, item_pos) {
                Some(avg) => 0.8 * avg + 0.2 * cur,
                None => cur,
            }
        } else {
            // Pass-through dummy wires stay anchored primarily toward their destination column so
            // long backup chains do not drift across dense intermediate fan-out hubs.
            let in_b = neighbor_barycenter(node, in_adj, item_pos);
            let out_b = neighbor_barycenter(node, out_adj, item_pos);
            match (in_b, out_b) {
                (Some(ib), Some(ob)) => 0.25 * ib + 0.75 * ob,
                (None, Some(ob)) => ob,
                (Some(ib), None) => ib,
                (None, None) => cur,
            }
        };
        scored.push((node, next));
    }
    scored.sort_by(|(na, sa), (nb, sb)| {
        sa.total_cmp(sb)
            .then_with(|| cmp_item_ord(item_ord[na], item_ord[nb]))
    });
    let sorted_nodes: Vec<LayerNode> = scored.into_iter().map(|(n, _)| n).collect();
    seed_column_positions(&sorted_nodes, max_span, item_pos);
    col_items.insert(col, sorted_nodes);
}

fn neighbor_barycenter(
    node: LayerNode,
    adj: &LayerAdjMap,
    pos_map: &BTreeMap<LayerNode, f64>,
) -> Option<f64> {
    let links = adj.get(&node)?;
    let mut ws = 0.0;
    let mut tw = 0.0;
    for link in links {
        if let Some(&p) = pos_map.get(&link.node) {
            let w = f64::from(link.weight.max(1));
            ws += w * p;
            tw += w;
        }
    }
    (tw > 0.0).then(|| ws / tw)
}

fn assign_column_slots(
    col: usize,
    max_span: f64,
    col_items: &BTreeMap<usize, Vec<LayerNode>>,
    in_adj: &LayerAdjMap,
    out_adj: &LayerAdjMap,
    item_slot: &mut BTreeMap<LayerNode, f64>,
) {
    let Some(items) = col_items.get(&col) else {
        return;
    };
    if items.is_empty() {
        return;
    }
    let span = column_span(items);
    let mut cur_default = (max_span - span).max(0.0) / 2.0;
    let mut ideals = Vec::with_capacity(items.len());

    let mut prev: Option<LayerNode> = None;
    for &node in items {
        if let Some(prev_node) = prev {
            cur_default += pair_gap(prev_node, node);
        }
        prev = Some(node);
        let mut ws = 0.0;
        let mut tw = 0.0;
        let mut main_nb: Option<f64> = None;
        for adj in [in_adj, out_adj] {
            if let Some(links) = adj.get(&node) {
                for link in links {
                    if let Some(&nb_s) = item_slot.get(&link.node) {
                        let w = f64::from(link.weight.max(1));
                        ws += w * nb_s;
                        tw += w;
                        if link.is_main {
                            main_nb = Some(nb_s);
                        }
                    }
                }
            }
        }
        let ideal = if tw > 0.0 {
            let avg = ws / tw;
            if let Some(m_slot) = main_nb
                && (avg - m_slot).abs() <= 0.28
            {
                m_slot
            } else {
                avg
            }
        } else {
            cur_default
        };
        ideals.push(ideal);
    }

    let slots = pav_variable_gaps(&ideals, items, max_span);
    let assigned = items.iter().copied().zip(slots);
    for (node, slot) in assigned {
        item_slot.insert(node, slot);
    }
}

/// Solves `min sum_i (y_i - ideal_i)^2` subject to `0 <= y_0`, `y_i - y_{i-1} >= gap_i`, and
/// `y_{k-1} <= max_span` in `O(k)` time via Pool-Adjacent-Violators isotonic regression.
fn pav_variable_gaps(ideals: &[f64], items: &[LayerNode], max_span: f64) -> Vec<f64> {
    let k = ideals.len();
    if k == 0 {
        return Vec::new();
    }
    let mut prefix = Vec::with_capacity(k);
    let mut acc = 0.0_f64;
    prefix.push(acc);
    let item_pairs = items.windows(2);
    for pair in item_pairs {
        acc += pair_gap(pair[0], pair[1]);
        prefix.push(acc);
    }
    let upper = (max_span - acc).max(0.0);
    let mut blocks: Vec<(f64, usize)> = Vec::with_capacity(k);

    let ideal_pairs = ideals.iter().copied().zip(prefix.iter().copied());
    for (ideal, pref) in ideal_pairs {
        let z = (ideal - pref).clamp(0.0, upper);
        blocks.push((z, 1));
        while blocks.len() >= 2 {
            let n = blocks.len();
            let (s1, c1) = blocks[n - 2];
            let (s2, c2) = blocks[n - 1];
            if s1 / usize_to_f64(c1) > s2 / usize_to_f64(c2) {
                blocks.pop();
                blocks[n - 2] = (s1 + s2, c1 + c2);
            } else {
                break;
            }
        }
    }

    let mut out = Vec::with_capacity(k);
    let mut pos_idx = 0_usize;
    for (sum_z, count) in blocks {
        let mean_z = (sum_z / usize_to_f64(count)).clamp(0.0, upper);
        for _ in 0..count {
            out.push(mean_z + prefix[pos_idx]);
            pos_idx += 1;
        }
    }
    out
}

fn compute_tree_metrics(
    asn: u32,
    children_of: &BTreeMap<u32, BTreeSet<u32>>,
    pair_weight: &BTreeMap<(u32, u32), u64>,
    leaves_memo: &mut BTreeMap<u32, usize>,
    weight_memo: &mut BTreeMap<u32, u64>,
) {
    if leaves_memo.contains_key(&asn) {
        return;
    }
    enum Frame {
        Enter(u32),
        Exit(u32),
    }
    let mut on_path: BTreeSet<u32> = BTreeSet::new();
    let mut stack = vec![Frame::Enter(asn)];
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Enter(node) => {
                if leaves_memo.contains_key(&node) {
                    continue;
                }
                let Some(children) = children_of.get(&node) else {
                    leaves_memo.insert(node, 1);
                    weight_memo.insert(node, 0);
                    continue;
                };
                if children.is_empty() {
                    leaves_memo.insert(node, 1);
                    weight_memo.insert(node, 0);
                    continue;
                }
                on_path.insert(node);
                stack.push(Frame::Exit(node));
                let child_vec: Vec<u32> = children.iter().copied().collect();
                let mut idx = child_vec.len();
                while idx > 0 {
                    idx -= 1;
                    let child = child_vec[idx];
                    if !leaves_memo.contains_key(&child) && !on_path.contains(&child) {
                        stack.push(Frame::Enter(child));
                    }
                }
            }
            Frame::Exit(node) => {
                on_path.remove(&node);
                let mut sum_leaves = 0_usize;
                let mut sum_weight = 0_u64;
                if let Some(children) = children_of.get(&node) {
                    for &child in children {
                        sum_leaves = sum_leaves
                            .saturating_add(leaves_memo.get(&child).copied().unwrap_or(1));
                        let edge_w = pair_weight.get(&(node, child)).copied().unwrap_or(0);
                        let sub_w = weight_memo.get(&child).copied().unwrap_or(0);
                        sum_weight = sum_weight.saturating_add(edge_w).saturating_add(sub_w);
                    }
                }
                leaves_memo.insert(node, sum_leaves.max(1));
                weight_memo.insert(node, sum_weight);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dfs_order(
    asn: u32,
    children_of: &BTreeMap<u32, BTreeSet<u32>>,
    pair_weight: &BTreeMap<(u32, u32), u64>,
    subtree_leaves: &BTreeMap<u32, usize>,
    subtree_weight: &BTreeMap<u32, u64>,
    seen: &mut BTreeSet<u32>,
    ord: &mut BTreeMap<u32, f64>,
    leaf_counter: &mut usize,
) -> f64 {
    if seen.contains(&asn) {
        return ord.get(&asn).copied().unwrap_or(0.0);
    }
    enum DfsFrame {
        Enter(u32),
        Exit {
            node: u32,
            all: Vec<u32>,
            centered: Vec<u32>,
        },
    }
    let mut stack = vec![DfsFrame::Enter(asn)];
    while let Some(frame) = stack.pop() {
        match frame {
            DfsFrame::Enter(node) => {
                if seen.contains(&node) {
                    continue;
                }
                seen.insert(node);
                let all: Vec<u32> = children_of
                    .get(&node)
                    .map(|set| set.iter().copied().collect())
                    .unwrap_or_default();
                if all.is_empty() {
                    let val = usize_to_f64(*leaf_counter);
                    *leaf_counter += 1;
                    ord.insert(node, val);
                    continue;
                }
                // Sort unvisited children by total branch peer weight descending so the dominant
                // trunk sits in the vertical center while minor backup branches fan out.
                let mut unvisited: Vec<u32> = Vec::new();
                for &k in &all {
                    if !seen.contains(&k) {
                        unvisited.push(k);
                    }
                }
                unvisited.sort_by(|a, b| {
                    let wa = pair_weight
                        .get(&(node, *a))
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(subtree_weight.get(a).copied().unwrap_or(0));
                    let wb = pair_weight
                        .get(&(node, *b))
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(subtree_weight.get(b).copied().unwrap_or(0));
                    wb.cmp(&wa)
                        .then_with(|| subtree_leaves[b].cmp(&subtree_leaves[a]))
                        .then_with(|| a.cmp(b))
                });
                let mut centered_dq: VecDeque<u32> = VecDeque::with_capacity(unvisited.len());
                let indexed_unvisited = unvisited.iter().copied().enumerate();
                for (idx, child) in indexed_unvisited {
                    if idx % 2 == 1 {
                        centered_dq.push_back(child);
                    } else {
                        centered_dq.push_front(child);
                    }
                }
                let centered: Vec<u32> = centered_dq.into_iter().collect();
                stack.push(DfsFrame::Exit {
                    node,
                    all,
                    centered: centered.clone(),
                });
                let mut idx = centered.len();
                while idx > 0 {
                    idx -= 1;
                    stack.push(DfsFrame::Enter(centered[idx]));
                }
            }
            DfsFrame::Exit {
                node,
                all,
                centered,
            } => {
                let mut values = Vec::with_capacity(all.len());
                for &child in &centered {
                    values.push(ord.get(&child).copied().unwrap_or(0.0));
                }
                for &child in &all {
                    if !centered.contains(&child)
                        && let Some(&existing) = ord.get(&child)
                    {
                        values.push(existing);
                    }
                }
                let mean = if values.is_empty() {
                    let v = usize_to_f64(*leaf_counter);
                    *leaf_counter += 1;
                    v
                } else {
                    let sum: f64 = values.iter().sum();
                    sum / usize_to_f64(values.len())
                };
                ord.insert(node, mean);
            }
        }
    }
    ord.get(&asn).copied().unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// 1. Self-contained inline-styled SVG renderer (`--format text` upload & `--format json`)
// ---------------------------------------------------------------------------

const SVG_NW: f64 = 120.0;
const SVG_NH: f64 = 34.0;
const SVG_COLG: f64 = 56.0;
const SVG_ROWG: f64 = 14.0;
const SVG_HEAD: f64 = 26.0;

#[allow(clippy::cast_precision_loss)]
const fn usize_to_f64(value: usize) -> f64 {
    value as f64
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn f64_to_u32_sat(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        value as u32
    }
}

/// Renders `graph` as a standalone UTF-8 SVG document with baked inline styles, matching the
/// visual geometry of `peer.as`.
#[must_use]
pub fn render_svg(graph: &PositionedGraph) -> String {
    let col_p = SVG_NW + SVG_COLG;
    let row_p = SVG_NH + SVG_ROWG;
    let cx = |col: usize| SVG_COLG + usize_to_f64(col) * col_p + SVG_NW / 2.0;
    let cy = |slot: f64| SVG_HEAD + SVG_ROWG / 2.0 + slot * row_p + SVG_NH / 2.0;

    let width = usize_to_f64(graph.max_col + 1) * col_p + SVG_COLG;
    let height = SVG_HEAD + SVG_ROWG + (graph.max_slot + 1.0) * row_p;
    let w_int = f64_to_u32_sat(width.round());
    let h_int = f64_to_u32_sat(height.round());

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

    // Cubic Bezier edges (threading through unobstructed waypoints for multi-column spans).
    let mut badges: Vec<(f64, f64, usize, bool)> = Vec::new();
    for edge in &graph.edges {
        let mut pts: Vec<(f64, f64)> = Vec::with_capacity(edge.waypoints.len() + 2);
        pts.push((cx(edge.from_col) + SVG_NW / 2.0, cy(edge.from_slot)));
        for &(wp_col, wp_slot) in &edge.waypoints {
            pts.push((cx(wp_col), cy(wp_slot)));
        }
        pts.push((cx(edge.to_col) - SVG_NW / 2.0, cy(edge.to_slot)));

        let mut path_d = format!("M{:.1},{:.1}", pts[0].0, pts[0].1);
        let pt_pairs = pts.windows(2);
        for pair in pt_pairs {
            let (px1, py1) = pair[0];
            let (px2, py2) = pair[1];
            let mx = (px1 + px2) / 2.0;
            let _ = write!(
                path_d,
                " C{mx:.1},{py1:.1} {mx:.1},{py2:.1} {px2:.1},{py2:.1}"
            );
        }

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
            "  <path d=\"{path_d}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{sw:.2}\" \
             opacity=\"{opacity}\"{dash}/>"
        );
        if edge.span >= 2 {
            let mid = pts.len() / 2;
            let (bx, by) = if pts.len() % 2 == 1 {
                pts[mid]
            } else {
                let (px1, py1) = pts[mid - 1];
                let (px2, py2) = pts[mid];
                ((px1 + px2) / 2.0, (py1 + py2) / 2.0)
            };
            badges.push((bx, by, edge.span, edge.is_main));
        }
    }

    // Pill badges for `×N` prepends rendered above edge strokes so lines never strike through text.
    for (bx, by, span, is_main) in badges {
        let digits = span.to_string().len();
        let pill_w = 18.0 + 6.0 * usize_to_f64(digits);
        let pill_x = bx - pill_w / 2.0;
        let pill_y = by - 8.0;
        let b_stroke = if is_main { "#38bdf8" } else { "#475569" };
        let _ = writeln!(
            out,
            "  <rect x=\"{pill_x:.1}\" y=\"{pill_y:.1}\" width=\"{pill_w:.1}\" height=\"16\" \
             rx=\"4\" fill=\"#0b1220\" stroke=\"{b_stroke}\" stroke-width=\"1\"/>"
        );
        let text_y = by + 0.5;
        let _ = writeln!(
            out,
            "  <text x=\"{bx:.1}\" y=\"{text_y:.1}\" fill=\"#cbd5e1\" \
             font-family=\"ui-monospace,monospace\" font-size=\"10\" font-weight=\"700\" \
             text-anchor=\"middle\" dominant-baseline=\"middle\">&#xD7;{span}</text>"
        );
    }

    // ASN boxes.
    for b in &graph.boxes {
        let x = cx(b.col);
        let y = cy(b.y_slot);
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
        let clean_name = strip_disallowed_xml_chars(&b.name);
        let short_name = truncate_display_width(clean_name.trim(), BOX_SUBTITLE_WIDTH);
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
            '\t' | '\n' | '\r' => out.push(' '),
            c if is_disallowed_xml_char(c) => {}
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
        let clean_name = strip_disallowed_xml_chars(&b.name);
        let short = truncate_display_width(clean_name.trim(), BOX_SUBTITLE_WIDTH);
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

    let indexed_links = graph.edges.iter().enumerate();
    for (link_idx, edge) in indexed_links {
        if edge.is_main {
            let _ = writeln!(
                out,
                "  linkStyle {link_idx} stroke:#38bdf8,stroke-width:2.8px"
            );
        } else {
            let _ = writeln!(
                out,
                "  linkStyle {link_idx} stroke:#64748b,stroke-width:1.5px,opacity:0.65"
            );
        }
    }

    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn sanitize_mermaid_label(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '"' | '`' | '\\' => Some('\''),
            '\r' | '\n' | '\t' => Some(' '),
            other if is_disallowed_xml_char(other) => None,
            other => Some(other),
        })
        .collect()
}

fn strip_disallowed_xml_chars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    consume_xml_csi(&mut chars);
                }
                Some(']' | 'P' | 'X' | '^' | '_') => {
                    chars.next();
                    consume_xml_st(&mut chars, &mut out);
                }
                _ => {}
            },
            '\u{009b}' => consume_xml_csi(&mut chars),
            '\u{0090}' | '\u{0098}' | '\u{009d}' | '\u{009e}' | '\u{009f}' => {
                consume_xml_st(&mut chars, &mut out);
            }
            '\r' | '\n' | '\t' => out.push(' '),
            other if is_disallowed_xml_char(other) => {}
            other => out.push(other),
        }
    }
    out
}

fn consume_xml_csi<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) {
    while let Some(&ch) = chars.peek() {
        if ('\x20'..='\x3f').contains(&ch) {
            chars.next();
        } else {
            break;
        }
    }
    if let Some(&fin) = chars.peek()
        && ('\x40'..='\x7e').contains(&fin)
    {
        chars.next();
    }
}

fn consume_xml_st<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>, out: &mut String) {
    while let Some(ch) = chars.next() {
        match ch {
            '\x07' | '\u{009c}' => break,
            '\x1b' => {
                if chars.peek() == Some(&'\\') {
                    chars.next();
                }
                break;
            }
            '\n' => {
                out.push(' ');
                break;
            }
            _ => {}
        }
    }
}

fn is_disallowed_xml_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{fffe}' | '\u{ffff}'
        )
}

const fn char_display_width(ch: char) -> usize {
    if ch > '\u{2e7f}' { 2 } else { 1 }
}

fn truncate_display_width(text: &str, max_width: usize) -> String {
    let mut total = 0_usize;
    for ch in text.chars() {
        total += char_display_width(ch);
    }
    if total <= max_width {
        return text.to_owned();
    }
    let limit = max_width.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0_usize;
    for ch in text.chars() {
        let w = char_display_width(ch);
        if used + w > limit {
            break;
        }
        out.push(ch);
        used += w;
    }
    let trimmed = out.trim_end_matches([' ', '-', '·', '.', ',']);
    format!("{trimmed}…")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use plugin_network::bgp::BgpPath;
    use plugin_network::bgp::FamilyBgp;
    use plugin_network::bgp::OriginSummary;
    use plugin_network::bgp::RpkiStatus;
    use plugin_network::bgp::UpstreamShare;

    use super::layout;
    use super::render_mermaid;
    use super::render_svg;

    #[test]
    fn svg_and_mermaid_strip_control_and_noncharacter_codepoints_from_asn_labels() {
        let mut asn_names = BTreeMap::new();
        asn_names.insert(
            64500,
            "\0\x07\x1b<\"Bad\">\x7f\u{0085}\u{fffe}\u{ffff}\nCo".to_owned(),
        );
        let rep = FamilyBgp {
            address: "203.0.113.7".parse().expect("ipv4"),
            announced: true,
            prefix: Some("203.0.113.0/24".to_owned()),
            prefix_base: Some("203.0.113.0".parse().expect("base")),
            prefix_len: Some(24),
            origin_asn: Some(64500),
            origins: vec![OriginSummary {
                asn: 64500,
                peers: 5,
            }],
            rpki: RpkiStatus::Valid,
            total_peers: 5,
            distinct_paths: 1,
            upstreams: vec![UpstreamShare {
                asn: 3356,
                peers: 5,
            }],
            paths: vec![BgpPath {
                asns: vec![3356, 64500],
                asns_raw: vec![3356, 64500],
                peers: 5,
                is_best: true,
            }],
            asn_names,
            diagram_url: None,
        };
        let graph = layout(&rep, None).expect("layout succeeds");
        let svg = render_svg(&graph);
        let mermaid = render_mermaid(&graph);

        assert!(
            svg.contains("&lt;&quot;Bad&quot;&gt; Co"),
            "SVG escapes XML entities and strips control characters:\n{svg}"
        );
        assert!(
            !svg.chars()
                .any(|c| (c.is_control() && c != '\n') || matches!(c, '\u{fffe}' | '\u{ffff}')),
            "SVG must contain no illegal XML 1.0 characters:\n{svg}"
        );
        assert!(
            mermaid.contains("<'Bad'> Co"),
            "Mermaid sanitizes quotes and strips control characters:\n{mermaid}"
        );
        assert!(
            !mermaid
                .chars()
                .any(|c| (c.is_control() && c != '\n') || matches!(c, '\u{fffe}' | '\u{ffff}')),
            "Mermaid must contain no control characters:\n{mermaid}"
        );
    }

    #[test]
    fn tier1_self_prepend_and_deep_as_path_layout_without_stack_overflow() {
        let mut asn_names = BTreeMap::new();
        asn_names.insert(
            6939,
            "Hurricane\x1b[31mEvil\x1b[0m\x1b]52;c;SGVsbG8=\x07\u{202e}".to_owned(),
        );
        let deep_chain: Vec<u32> = std::iter::once(3356)
            .chain((10_000..15_000).rev())
            .collect();
        let rep = FamilyBgp {
            address: "203.0.113.7".parse().expect("ipv4"),
            announced: true,
            prefix: Some("203.0.113.0/24".to_owned()),
            prefix_base: Some("203.0.113.0".parse().expect("base")),
            prefix_len: Some(24),
            origin_asn: Some(6939),
            origins: vec![OriginSummary {
                asn: 6939,
                peers: 10,
            }],
            rpki: RpkiStatus::Valid,
            total_peers: 11,
            distinct_paths: 2,
            upstreams: vec![UpstreamShare {
                asn: 1299,
                peers: 10,
            }],
            paths: vec![
                BgpPath {
                    asns: vec![1299, 6939],
                    asns_raw: vec![1299, 6939, 6939],
                    peers: 10,
                    is_best: true,
                },
                BgpPath {
                    asns: deep_chain.clone(),
                    asns_raw: deep_chain,
                    peers: 1,
                    is_best: false,
                },
            ],
            asn_names,
            diagram_url: None,
        };
        let graph = layout(&rep, None).expect("layout succeeds");
        assert!(
            graph.edges.iter().any(|e| {
                e.from_asn == 6939
                    && e.from_col == 0
                    && e.to_asn == 1299
                    && e.to_col == 2
                    && e.span == 2
            }),
            "Tier-1 origin self-prepend must place upstream at col 2 with ×2 badge"
        );
        let svg = render_svg(&graph);
        assert!(svg.contains("HurricaneEvil"));
        assert!(!svg.contains("31m") && !svg.contains('\u{202e}'));
    }
}
