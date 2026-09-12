//! Deterministic detection core for schema formation (spec 0002): behavioral
//! overlays, Louvain community detection, cluster density.
//!
//! Everything here is scan-time and pure: overlays are computed from raw
//! signals and merged with the declared graph into a weighted undirected
//! union adjacency, never persisted as `context_links` rows. Determinism is
//! the contract (AC-2, decision #72): nodes are sorted, iteration order is
//! fixed, tie-breaks go to the smallest community id, and no wall-clock, RNG,
//! or hash-map iteration order influences any output.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use rusqlite::Connection;

use super::model::{load, Graph, NodeKey};

/// Co-observation count at which an overlay layer reaches full strength.
/// Sweep constant owned by the dogfood gate; below the cap a pair's layer
/// weight ramps linearly (`count / CO_CAP`), above it the curve saturates.
pub const CO_CAP: usize = 5;

/// Local-moving passes per level. Convergence almost always wins well
/// before this; the bound keeps the protocol fixed under pathological input.
const MAX_PASSES: usize = 20;

/// Moves require a strictly positive modularity improvement beyond noise.
const EPS: f64 = 1e-12;

/// Per-layer overlay weights applied on top of the curve. Launch defaults
/// are uniform; the dogfood gate freezes tuned values (spec Architecture 4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayWeights {
    pub co_commit: f64,
    pub co_anchor: f64,
    pub co_retrieval: f64,
}

impl Default for OverlayWeights {
    fn default() -> Self {
        Self {
            co_commit: 1.0,
            co_anchor: 1.0,
            co_retrieval: 1.0,
        }
    }
}

/// The normalize-and-cap curve shared by every overlay layer: a pair's
/// co-observation count becomes a weight in `[(0.0), 1.0]`, full strength at
/// `CO_CAP`. Count 0 yields 0.0, so empty telemetry contributes zero edges
/// by construction.
pub fn co_weight(count: usize) -> f64 {
    count.min(CO_CAP) as f64 / CO_CAP as f64
}

/// Weighted undirected union adjacency over a canonical node ordering.
/// `nodes` is sorted, `adj[i]` is sorted by neighbor index with no
/// duplicates and no zero-weight entries.
pub struct UnionGraph {
    pub nodes: Vec<NodeKey>,
    pub adj: Vec<Vec<(usize, f64)>>,
}

impl UnionGraph {
    /// Canonical constructor: sorts the node universe and aggregates pair
    /// weights into symmetric, sorted adjacency lists. Zero-weight pairs are
    /// dropped (empty telemetry must not materialize edges).
    pub fn build(mut nodes: Vec<NodeKey>, pairs: BTreeMap<(usize, usize), f64>) -> UnionGraph {
        nodes.sort();
        nodes.dedup();
        let mut adj = vec![Vec::new(); nodes.len()];
        for ((a, b), w) in pairs {
            if w <= 0.0 || a >= nodes.len() || b >= nodes.len() || a == b {
                continue;
            }
            adj[a].push((b, w));
            adj[b].push((a, w));
        }
        for nbrs in &mut adj {
            nbrs.sort_by_key(|(i, _)| *i);
        }
        UnionGraph { nodes, adj }
    }
}

/// Declared edges plus behavioral overlays, computed at scan time.
///
/// Layers: declared (weights summed per unordered pair from the graph
/// projection), co-commit (items sharing a `commit_sha` across decisions and
/// progress entries), co-anchor (items sharing an anchor path), co-retrieval
/// (items co-surfaced in one `retrieval_surfaces` event, keyed by
/// `(ts, cmd, arg)`). Each overlay layer contributes `weight * co_weight(pair
/// count)` on top of the declared weight.
pub fn union_graph(
    conn: &Connection,
    base: &Graph,
    weights: &OverlayWeights,
) -> Result<UnionGraph> {
    let commit_groups = co_commit_groups(conn)?;
    let anchor_groups = co_anchor_groups(conn)?;
    let surface_groups = co_retrieval_groups(conn)?;

    let mut keys: Vec<NodeKey> = base.nodes.clone();
    // Overlay ghosts stay out of the node universe: a group key naming a
    // node absent from the base projection (kind/id with no backing row,
    // e.g. a retrieval_surfaces event naming a since-deleted item) must not
    // materialize as a node. The rolling-window prune bounds their history;
    // this filter bounds the present.
    for groups in [&commit_groups, &anchor_groups, &surface_groups] {
        for group in groups {
            keys.extend(group.iter().filter(|k| base.contains(k)).cloned());
        }
    }
    keys.sort();
    keys.dedup();
    let index: HashMap<NodeKey, usize> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), i))
        .collect();
    let mut pairs: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    // Declared edges are interned in model::load's table-then-rowid order,
    // NOT in the union's sorted position; translate both endpoints through
    // the NodeKey→union index or they attach to the wrong pair whenever the
    // two orderings diverge (any workspace with mixed link kinds or
    // non-rowid-ordered ids).
    for e in &base.edges {
        let (Some(&a), Some(&b)) = (index.get(&base.nodes[e.src]), index.get(&base.nodes[e.tgt]))
        else {
            continue;
        };
        if a == b {
            continue;
        }
        *pairs.entry((a.min(b), a.max(b))).or_insert(0.0) += e.weight;
    }
    add_layer(&mut pairs, &commit_groups, &index, weights.co_commit);
    add_layer(&mut pairs, &anchor_groups, &index, weights.co_anchor);
    add_layer(&mut pairs, &surface_groups, &index, weights.co_retrieval);
    Ok(UnionGraph::build(keys, pairs))
}

/// Accumulate one overlay layer: every unordered pair co-observed `count`
/// times gains `layer_weight * co_weight(count)`.
fn add_layer(
    pairs: &mut BTreeMap<(usize, usize), f64>,
    groups: &[Vec<NodeKey>],
    index: &HashMap<NodeKey, usize>,
    layer_weight: f64,
) {
    if layer_weight <= 0.0 {
        return;
    }
    let mut counts: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for group in groups {
        for i in 0..group.len() {
            for j in i + 1..group.len() {
                let (Some(&a), Some(&b)) = (index.get(&group[i]), index.get(&group[j])) else {
                    continue;
                };
                if a != b {
                    *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
                }
            }
        }
    }
    for (pair, count) in counts {
        *pairs.entry(pair).or_insert(0.0) += layer_weight * co_weight(count);
    }
}

fn co_commit_groups(conn: &Connection) -> Result<Vec<Vec<NodeKey>>> {
    let mut groups: BTreeMap<String, Vec<NodeKey>> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT commit_sha, id FROM decisions WHERE commit_sha IS NOT NULL \
         ORDER BY commit_sha, id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for r in rows {
        let (sha, id) = r?;
        groups
            .entry(sha)
            .or_default()
            .push(("decision".to_string(), id.to_string()));
    }
    let mut stmt = conn.prepare(
        "SELECT commit_sha, id FROM progress_entries WHERE commit_sha IS NOT NULL \
         ORDER BY commit_sha, id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for r in rows {
        let (sha, id) = r?;
        groups
            .entry(sha)
            .or_default()
            .push(("progress_entry".to_string(), id.to_string()));
    }
    Ok(groups.into_values().collect())
}

fn co_anchor_groups(conn: &Connection) -> Result<Vec<Vec<NodeKey>>> {
    let mut groups: BTreeMap<String, Vec<NodeKey>> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT item_type, item_id, path FROM item_anchors \
         ORDER BY path, item_type, item_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for r in rows {
        let (item_type, id, path) = r?;
        groups
            .entry(path)
            .or_default()
            .push((canonical_kind(&item_type).to_string(), id.to_string()));
    }
    Ok(groups.into_values().collect())
}

fn co_retrieval_groups(conn: &Connection) -> Result<Vec<Vec<NodeKey>>> {
    let mut groups: BTreeMap<(String, String, Option<String>), Vec<NodeKey>> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT ts, cmd, arg, node_kind, node_id FROM retrieval_surfaces \
         ORDER BY ts, cmd, arg, node_kind, node_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    for r in rows {
        let (ts, cmd, arg, node_kind, node_id) = r?;
        groups
            .entry((ts, cmd, arg))
            .or_default()
            .push((canonical_kind(&node_kind).to_string(), node_id.to_string()));
    }
    Ok(groups.into_values().collect())
}

/// Map external kind spellings onto the graph projection's node kinds
/// (mirrors `super::parse_node`).
fn canonical_kind(raw: &str) -> &str {
    match raw {
        "pattern" | "system-pattern" | "system_pattern" => "system_pattern",
        "progress-entry" | "progress_entry" => "progress_entry",
        "custom-data" | "custom_data" => "custom_data",
        other => other,
    }
}

/// A detected community: members in canonical node order plus the weighted
/// subset density that feeds the AC-3 gate.
#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    pub members: Vec<NodeKey>,
    pub density: f64,
}

/// End-to-end detection: load the declared graph, merge overlays, run
/// deterministic Louvain, and return multi-member clusters ordered by their
/// smallest member. Singletons carry no community and are skipped.
pub fn clusters(conn: &Connection, weights: &OverlayWeights) -> Result<Vec<Cluster>> {
    let base = load(conn)?;
    let g = union_graph(conn, &base, weights)?;
    let labels = detect(&g);
    let Some(&max) = labels.iter().max() else {
        return Ok(Vec::new());
    };
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); max + 1];
    for (i, &c) in labels.iter().enumerate() {
        groups[c].push(i);
    }
    let mut out = Vec::new();
    for group in groups {
        if group.len() < 2 {
            continue;
        }
        let density = cluster_density(&g, &group);
        out.push(Cluster {
            members: group.iter().map(|&i| g.nodes[i].clone()).collect(),
            density,
        });
    }
    Ok(out)
}

/// Community label per node from deterministic Louvain: single-node
/// local-moving passes to convergence, then one level of collapse and
/// community-level passes, expanded back. Labels are canonical — renumbered
/// by first appearance in node order — so identical graphs yield identical
/// label vectors.
pub fn detect(g: &UnionGraph) -> Vec<usize> {
    let mut mover = LocalMover::new(&g.adj);
    if mover.m2 <= 0.0 {
        return (0..g.adj.len()).collect();
    }
    mover.run_to_convergence();
    let node_comm = mover.comm.clone();

    // One-level collapse: super nodes are communities in first-appearance
    // order; external edges aggregate, and each community's internal mass is
    // retained as a self-loop so the community level cannot merge two dense
    // groups across a weak bridge (without it, any bare connection is a
    // modularity-positive merge).
    let mut super_of: HashMap<usize, usize> = HashMap::new();
    let mut order: Vec<usize> = Vec::new();
    for &c in &node_comm {
        if let Entry::Vacant(e) = super_of.entry(c) {
            e.insert(order.len());
            order.push(c);
        }
    }
    let mut super_pairs: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for (i, nbrs) in g.adj.iter().enumerate() {
        for &(j, w) in nbrs {
            let a = super_of[&node_comm[i]];
            let b = super_of[&node_comm[j]];
            let key = if a <= b { (a, b) } else { (b, a) };
            *super_pairs.entry(key).or_insert(0.0) += w;
        }
    }
    let mut super_adj = vec![Vec::new(); order.len()];
    for ((a, b), w) in super_pairs {
        super_adj[a].push((b, w));
        if a != b {
            // Symmetric replication is for distinct supers only; a
            // self-loop is internal mass already fully counted in `w`.
            super_adj[b].push((a, w));
        }
    }
    for nbrs in &mut super_adj {
        nbrs.sort_by_key(|(i, _)| *i);
    }
    let mut super_mover = LocalMover::new(&super_adj);
    if super_mover.m2 > 0.0 {
        super_mover.run_to_convergence();
    }

    // Expand and canonicalize: relabel by first appearance in node order.
    let raw: Vec<usize> = node_comm
        .iter()
        .map(|c| super_mover.comm[super_of[c]])
        .collect();
    let mut canon: Vec<usize> = vec![usize::MAX; raw.len()];
    let mut next = 0;
    let mut out = Vec::with_capacity(raw.len());
    for &c in &raw {
        if canon[c] == usize::MAX {
            canon[c] = next;
            next += 1;
        }
        out.push(canon[c]);
    }
    out
}

/// Weighted density of a member subset: average pairwise strength over the
/// union adjacency, saturated to [0,1]. Edge weights are unit-bounded by
/// convention (declared default 1.0, overlay curve capped at 1.0), so count
/// weights saturate rather than skew. Fewer than two members → 0.0.
pub fn cluster_density(g: &UnionGraph, members: &[usize]) -> f64 {
    let mut ms = members.to_vec();
    ms.sort_unstable();
    ms.dedup();
    if ms.len() < 2 {
        return 0.0;
    }
    let pairs = ms.len() * (ms.len() - 1) / 2;
    let mut sum = 0.0;
    for (i, &a) in ms.iter().enumerate() {
        for &b in &ms[i + 1..] {
            sum += pair_strength(g, a, b);
        }
    }
    (sum / pairs as f64).clamp(0.0, 1.0)
}

/// Lookup of the aggregated weight between two node indices (0.0 if absent).
fn pair_strength(g: &UnionGraph, a: usize, b: usize) -> f64 {
    g.adj[a]
        .binary_search_by(|probe| probe.0.cmp(&b))
        .map(|i| g.adj[a][i].1)
        .unwrap_or(0.0)
}

/// Louvain local-moving state over one level of the hierarchy.
struct LocalMover<'a> {
    adj: &'a [Vec<(usize, f64)>],
    k: Vec<f64>,
    tot: Vec<f64>,
    m2: f64,
    comm: Vec<usize>,
    comm_w: Vec<f64>,
    touched: Vec<usize>,
}

impl<'a> LocalMover<'a> {
    fn new(adj: &'a [Vec<(usize, f64)>]) -> Self {
        let n = adj.len();
        let k: Vec<f64> = adj
            .iter()
            .map(|nbrs| nbrs.iter().map(|(_, w)| *w).sum())
            .collect();
        LocalMover {
            adj,
            tot: k.clone(),
            k,
            m2: adj
                .iter()
                .map(|nbrs| nbrs.iter().map(|(_, w)| *w).sum::<f64>())
                .sum(),
            comm: (0..n).collect(),
            comm_w: vec![0.0; n],
            touched: Vec::new(),
        }
    }

    fn run_to_convergence(&mut self) {
        for _ in 0..MAX_PASSES {
            if !self.pass() {
                break;
            }
        }
    }

    /// One full single-node local-moving pass in node-index order. A node
    /// moves only for a strictly positive gain (beyond `EPS`); ties among
    /// targets go to the smallest community id because `touched` is scanned
    /// ascending. Returns whether anything moved.
    fn pass(&mut self) -> bool {
        let mut moved = false;
        for i in 0..self.adj.len() {
            let old = self.comm[i];
            self.touched.clear();
            for &(j, w) in &self.adj[i] {
                let c = self.comm[j];
                if self.comm_w[c] == 0.0 {
                    self.touched.push(c);
                }
                self.comm_w[c] += w;
            }
            self.touched.sort_unstable();
            self.touched.dedup();
            // Remove i from its community, then score candidate communities:
            // gain(c) = w(i→c) - k(i)·tot(c)/m2 (standard Louvain form).
            self.tot[old] -= self.k[i];
            let mut best_c = old;
            let mut best_gain = self.comm_w[old] - self.k[i] * self.tot[old] / self.m2;
            for &c in &self.touched {
                let gain = self.comm_w[c] - self.k[i] * self.tot[c] / self.m2;
                if gain > best_gain + EPS {
                    best_gain = gain;
                    best_c = c;
                }
            }
            if best_c != old {
                self.comm[i] = best_c;
                self.tot[best_c] += self.k[i];
                moved = true;
            } else {
                self.tot[old] += self.k[i];
            }
            for &c in &self.touched {
                self.comm_w[c] = 0.0;
            }
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::SCHEMA).unwrap();
        conn
    }

    fn add_decision(conn: &Connection, id: i64, commit: Option<&str>) {
        conn.execute(
            "INSERT INTO decisions (uuid, timestamp, summary, commit_sha) \
             VALUES (?1, '2026-01-01T00:00:00Z', 'd', ?2)",
            rusqlite::params![format!("u{id}"), commit],
        )
        .unwrap();
    }

    fn add_progress(conn: &Connection, commit: Option<&str>) {
        conn.execute(
            "INSERT INTO progress_entries (timestamp, status, description, commit_sha) \
             VALUES ('2026-01-01T00:00:00Z', 'done', 'p', ?1)",
            rusqlite::params![commit],
        )
        .unwrap();
    }

    fn add_declared(conn: &Connection, a: i64, b: i64, weight: f64) {
        conn.execute(
            "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, \
             target_item_id, relationship_type, timestamp, weight, origin) \
             VALUES ('decision', ?1, 'decision', ?2, 'relates_to', \
             '2026-01-01T00:00:00Z', ?3, 'manual')",
            rusqlite::params![a.to_string(), b.to_string(), weight],
        )
        .unwrap();
    }

    fn dkey(id: i64) -> NodeKey {
        ("decision".to_string(), id.to_string())
    }

    /// Union position of a node key: `nodes` is sorted, so binary search is
    /// exact.
    fn gpos(g: &UnionGraph, key: &NodeKey) -> usize {
        g.nodes.binary_search(key).unwrap()
    }

    /// Clique on `ids` with unit weights plus optional weak bridge edge.
    fn two_cluster_graph() -> UnionGraph {
        let nodes: Vec<NodeKey> = (1..=8).map(dkey).collect();
        let mut pairs: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        let edge = |a: usize, b: usize, w: f64, pairs: &mut BTreeMap<(usize, usize), f64>| {
            pairs.insert((a.min(b), a.max(b)), w);
        };
        for i in 0..4 {
            for j in i + 1..4 {
                edge(i, j, 1.0, &mut pairs);
            }
        }
        for i in 4..8 {
            for j in i + 1..8 {
                edge(i, j, 1.0, &mut pairs);
            }
        }
        edge(3, 4, 0.2, &mut pairs);
        UnionGraph::build(nodes, pairs)
    }

    #[test]
    fn co_weight_ramps_to_cap() {
        assert_eq!(co_weight(0), 0.0);
        assert_eq!(co_weight(1), 0.2);
        assert_eq!(co_weight(CO_CAP), 1.0);
        assert_eq!(co_weight(CO_CAP + 10), 1.0);
    }

    #[test]
    fn union_merges_declared_and_overlay_layers() {
        let conn = mem_db();
        add_decision(&conn, 1, Some("abc"));
        add_decision(&conn, 2, Some("abc"));
        add_progress(&conn, Some("abc")); // same commit, cross-kind pair
        add_declared(&conn, 1, 2, 0.5);
        conn.execute(
            "INSERT INTO item_anchors (item_type, item_id, path, timestamp) \
             VALUES ('decision', 1, 'src/a.rs', 't'), ('decision', 2, 'src/a.rs', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
             VALUES ('t1', 'query', 'x', 'decision', 1), ('t1', 'query', 'x', 'decision', 2)",
            [],
        )
        .unwrap();

        let base = load(&conn).unwrap();
        let g = union_graph(&conn, &base, &OverlayWeights::default()).unwrap();
        // declared 0.5 + co-commit 0.2 + co-anchor 0.2 + co-retrieval 0.2
        let expected = 0.5 + 3.0 * co_weight(1);
        let i1 = gpos(&g, &dkey(1));
        let i2 = gpos(&g, &dkey(2));
        let ip = gpos(&g, &("progress_entry".to_string(), "1".to_string()));
        assert!((pair_strength(&g, i1, i2) - expected).abs() < 1e-9);
        // progress entry shares only the commit: co-commit alone.
        assert!((pair_strength(&g, i1, ip) - co_weight(1)).abs() < 1e-9);
    }

    #[test]
    fn empty_telemetry_contributes_zero_edges() {
        let conn = mem_db();
        add_decision(&conn, 1, None);
        add_decision(&conn, 2, None);
        add_declared(&conn, 1, 2, 1.0);
        let base = load(&conn).unwrap();
        let g = union_graph(&conn, &base, &OverlayWeights::default()).unwrap();
        let i1 = gpos(&g, &dkey(1));
        let i2 = gpos(&g, &dkey(2));
        assert_eq!(pair_strength(&g, i1, i2), 1.0);
        // No overlay endpoints or phantom edges beyond the declared one.
        let edge_count: usize = g.adj.iter().map(|n| n.len()).sum();
        assert_eq!(edge_count, 2);
    }

    #[test]
    fn repeated_co_observation_ramps_weight() {
        let conn = mem_db();
        add_decision(&conn, 1, None);
        add_decision(&conn, 2, None);
        // Co-surfaced in three separate events: count 3.
        for ts in ["t1", "t2", "t3"] {
            conn.execute(
                "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
                 VALUES (?1, 'query', 'x', 'decision', 1), (?1, 'query', 'x', 'decision', 2)",
                rusqlite::params![ts],
            )
            .unwrap();
        }
        let base = load(&conn).unwrap();
        let g = union_graph(&conn, &base, &OverlayWeights::default()).unwrap();
        let i1 = gpos(&g, &dkey(1));
        let i2 = gpos(&g, &dkey(2));
        assert!((pair_strength(&g, i1, i2) - co_weight(3)).abs() < 1e-9);
    }

    #[test]
    fn declared_edges_attach_to_true_endpoints_when_orders_diverge() {
        let conn = mem_db();
        // model::load interns decisions first by rowid, so inserting 2
        // before 1 plus later kinds makes base insertion order differ from
        // the union's sorted NodeKey order — exactly when reusing base
        // indices raw attaches declared edges to the wrong pair.
        add_decision(&conn, 2, None);
        add_decision(&conn, 1, None);
        conn.execute(
            "INSERT INTO system_patterns (uuid, timestamp, name) \
             VALUES ('sp1', '2026-01-01T00:00:00Z', 'p1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO custom_data (timestamp, category, key, value) \
             VALUES ('2026-01-01T00:00:00Z', 'cat', 'k', 'v')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO code_nodes (kind, path, symbol, first_seen, last_seen) \
             VALUES ('fn', 'src/a.rs', 'a', 't', 't')",
            [],
        )
        .unwrap();
        for (sa, si, ta, ti) in [
            ("decision", "1", "system_pattern", "1"),
            ("decision", "2", "custom_data", "1"),
        ] {
            conn.execute(
                "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, \
                 target_item_id, relationship_type, timestamp, weight, origin) \
                 VALUES (?1, ?2, ?3, ?4, 'relates_to', '2026-01-01T00:00:00Z', 1.0, 'manual')",
                rusqlite::params![sa, si, ta, ti],
            )
            .unwrap();
        }

        let base = load(&conn).unwrap();
        let g = union_graph(&conn, &base, &OverlayWeights::default()).unwrap();
        let key = |kind: &str, id: &str| (kind.to_string(), id.to_string());
        let strength = |a: &NodeKey, b: &NodeKey| pair_strength(&g, gpos(&g, a), gpos(&g, b));
        // The true endpoint pairs carry the declared weights.
        assert!((strength(&key("decision", "1"), &key("system_pattern", "1")) - 1.0).abs() < 1e-9);
        assert!((strength(&key("decision", "2"), &key("custom_data", "1")) - 1.0).abs() < 1e-9);
        // The pairs raw base-index reuse would have polluted stay empty.
        assert_eq!(
            strength(&key("custom_data", "1"), &key("decision", "1")),
            0.0
        );
        assert_eq!(strength(&key("code", "1"), &key("decision", "2")), 0.0);
    }

    #[test]
    fn surface_ghosts_do_not_extend_the_universe() {
        let conn = mem_db();
        add_decision(&conn, 1, None);
        add_decision(&conn, 2, None);
        add_declared(&conn, 1, 2, 1.0);
        // One co-surface event names decision 99, which has no backing row:
        // the ghost must vanish from the group (no node, no edges) while
        // the real pair still counts the event.
        conn.execute(
            "INSERT INTO retrieval_surfaces (ts, cmd, arg, node_kind, node_id) \
             VALUES ('t1', 'query', 'x', 'decision', 1), \
                    ('t1', 'query', 'x', 'decision', 2), \
                    ('t1', 'query', 'x', 'decision', 99)",
            [],
        )
        .unwrap();

        let base = load(&conn).unwrap();
        let g = union_graph(&conn, &base, &OverlayWeights::default()).unwrap();
        let expected = 1.0 + co_weight(1);
        assert!(
            (pair_strength(&g, gpos(&g, &dkey(1)), gpos(&g, &dkey(2))) - expected).abs() < 1e-9
        );
        assert!(!g
            .nodes
            .contains(&("decision".to_string(), "99".to_string())));
    }

    #[test]
    fn detects_two_clusters_with_weak_bridge() {
        let g = two_cluster_graph();
        let labels = detect(&g);
        assert_eq!(labels[0], labels[1]);
        assert_eq!(labels[1], labels[2]);
        assert_eq!(labels[2], labels[3]);
        assert_eq!(labels[4], labels[5]);
        assert_eq!(labels[5], labels[6]);
        assert_eq!(labels[6], labels[7]);
        assert_ne!(labels[0], labels[4]);
    }

    #[test]
    fn identical_labels_on_repeated_runs() {
        let g = two_cluster_graph();
        assert_eq!(detect(&g), detect(&g));
    }

    #[test]
    fn identical_clusters_on_repeated_db_runs() {
        let conn = mem_db();
        for (id, commit) in [
            (1, "s1"),
            (2, "s1"),
            (3, "s1"),
            (4, "s2"),
            (5, "s2"),
            (6, "s2"),
        ] {
            add_decision(&conn, id, Some(commit));
        }
        let first = clusters(&conn, &OverlayWeights::default()).unwrap();
        let second = clusters(&conn, &OverlayWeights::default()).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        let sig = |c: &Cluster| -> Vec<NodeKey> { c.members.clone() };
        assert_eq!(sig(&first[0]), vec![dkey(1), dkey(2), dkey(3)]);
        assert_eq!(sig(&first[1]), vec![dkey(4), dkey(5), dkey(6)]);
    }

    #[test]
    fn empty_graph_yields_no_clusters() {
        let conn = mem_db();
        assert!(clusters(&conn, &OverlayWeights::default())
            .unwrap()
            .is_empty());
        let g = UnionGraph::build(Vec::new(), BTreeMap::new());
        assert!(detect(&g).is_empty());
    }

    #[test]
    fn singleton_clusters_are_skipped() {
        let conn = mem_db();
        add_decision(&conn, 1, None);
        let out = clusters(&conn, &OverlayWeights::default()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn edgeless_graph_labels_singletons() {
        let nodes = vec![dkey(1), dkey(2), dkey(3)];
        let g = UnionGraph::build(nodes, BTreeMap::new());
        assert_eq!(detect(&g), vec![0, 1, 2]);
    }

    #[test]
    fn disconnected_components_stay_separate() {
        let nodes: Vec<NodeKey> = (1..=6).map(dkey).collect();
        let mut pairs: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        for (a, b) in [(0, 1), (1, 2), (0, 2), (3, 4), (4, 5), (3, 5)] {
            pairs.insert((a, b), 1.0);
        }
        let g = UnionGraph::build(nodes, pairs);
        let labels = detect(&g);
        assert_eq!(labels[0], labels[1]);
        assert_eq!(labels[1], labels[2]);
        assert_eq!(labels[3], labels[4]);
        assert_eq!(labels[4], labels[5]);
        assert_ne!(labels[0], labels[3]);
    }

    #[test]
    fn dense_cluster_density_high_hub_spoke_low() {
        // Dense 4-clique: every pair at unit strength.
        let nodes: Vec<NodeKey> = (1..=4).map(dkey).collect();
        let mut pairs: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        for i in 0..4 {
            for j in i + 1..4 {
                pairs.insert((i, j), 1.0);
            }
        }
        let dense = UnionGraph::build(nodes, pairs);
        assert!(cluster_density(&dense, &[0, 1, 2, 3]) > 0.99);

        // Hub-spoke: hub pairs strong, leaf pairs absent.
        let nodes: Vec<NodeKey> = (1..=5).map(dkey).collect();
        let mut pairs: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        for leaf in 1..5 {
            pairs.insert((0, leaf), 1.0);
        }
        let star = UnionGraph::build(nodes, pairs);
        let density = cluster_density(&star, &[0, 1, 2, 3, 4]);
        assert!((density - 0.4).abs() < 1e-9, "hub-spoke density {density}");
        // The star still forms one community; the density gate is what
        // rejects it (hub detection quality lives downstream).
        assert_eq!(detect(&star), vec![0, 0, 0, 0, 0]);
    }

    #[test]
    fn single_member_density_zero() {
        let g = two_cluster_graph();
        assert_eq!(cluster_density(&g, &[0]), 0.0);
        assert_eq!(cluster_density(&g, &[]), 0.0);
    }
}
