//! In-memory graph model + analytics.
//!
//! The knowledge base is tiny (KBs), so the whole graph is loaded and
//! analytics are computed in memory. Two projections are kept: a weighted
//! **undirected** adjacency (degree, components, path, neighbors, density)
//! and the raw **directed** edge list (PageRank). Symmetric relationships
//! (per `rel::is_symmetric`) contribute both directions to PageRank.

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use super::rel;

pub type NodeKey = (String, String); // (kind, id)

pub struct Edge {
    pub src: usize,
    pub tgt: usize,
    pub rel: String,
    pub weight: f64,
    pub symmetric: bool,
    pub origin: String,
}

pub struct Graph {
    pub nodes: Vec<NodeKey>,
    index: HashMap<NodeKey, usize>,
    pub edges: Vec<Edge>,
    undirected: Vec<Vec<(usize, f64, String)>>, // (neighbor, weight, rel)
}

fn intern(nodes: &mut Vec<NodeKey>, index: &mut HashMap<NodeKey, usize>, key: NodeKey) -> usize {
    if let Some(&i) = index.get(&key) {
        return i;
    }
    let i = nodes.len();
    nodes.push(key.clone());
    index.insert(key, i);
    i
}

pub fn fmt_node(key: &NodeKey) -> String {
    format!("{}:{}", key.0, key.1)
}

pub fn load(conn: &Connection) -> Result<Graph> {
    let mut nodes: Vec<NodeKey> = Vec::new();
    let mut index: HashMap<NodeKey, usize> = HashMap::new();

    // Node universe: every knowledge item + code node.
    for (table, kind) in [
        ("decisions", "decision"),
        ("system_patterns", "system_pattern"),
        ("progress_entries", "progress_entry"),
        ("custom_data", "custom_data"),
        ("code_nodes", "code"),
    ] {
        let mut stmt = conn.prepare(&format!("SELECT id FROM {}", table))?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        for r in rows {
            intern(&mut nodes, &mut index, (kind.to_string(), r?.to_string()));
        }
    }

    // Edges: all context_links (manual + derived). Endpoints not in the
    // tables above (e.g. `pr:<id>`) are interned on first sight.
    let mut edges = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT source_item_type, source_item_id, target_item_type, target_item_id, \
             relationship_type, weight, origin FROM context_links ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;
        for r in rows {
            let (st, si, tt, ti, relationship, weight, origin) = r?;
            let src = intern(&mut nodes, &mut index, (st, si));
            let tgt = intern(&mut nodes, &mut index, (tt, ti));
            edges.push(Edge {
                src,
                tgt,
                symmetric: rel::is_symmetric(&relationship),
                rel: relationship,
                weight,
                origin,
            });
        }
    }

    let mut undirected = vec![Vec::new(); nodes.len()];
    for e in &edges {
        undirected[e.src].push((e.tgt, e.weight, e.rel.clone()));
        if e.src != e.tgt {
            undirected[e.tgt].push((e.src, e.weight, e.rel.clone()));
        }
    }

    Ok(Graph {
        nodes,
        index,
        edges,
        undirected,
    })
}

impl Graph {
    /// Membership probe for the overlay ghost filter: true when `key` is
    /// in this graph's node universe.
    pub fn contains(&self, key: &NodeKey) -> bool {
        self.index.contains_key(key)
    }

    /// Weighted degree per node (sum of incident undirected edge weights).
    /// Empty sums are normalized: Rust's `sum()` yields `-0.0` for empty iterators.
    pub fn degree(&self) -> Vec<f64> {
        self.undirected
            .iter()
            .map(|nbrs| {
                let s: f64 = nbrs.iter().map(|(_, w, _)| w).sum();
                if s == 0.0 {
                    0.0
                } else {
                    s
                }
            })
            .collect()
    }

    /// PageRank over the directed edge list; symmetric rels contribute both
    /// directions. Damping 0.85, ≤100 iterations, L1 stop < 1e-6, dangling
    /// mass redistributed uniformly. Empty graph → empty vec.
    pub fn pagerank(&self) -> Vec<f64> {
        let n = self.nodes.len();
        if n == 0 {
            return Vec::new();
        }
        let mut out: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for e in &self.edges {
            out[e.src].push((e.tgt, e.weight));
            if e.symmetric && e.src != e.tgt {
                out[e.tgt].push((e.src, e.weight));
            }
        }
        let out_sum: Vec<f64> = out
            .iter()
            .map(|v| v.iter().map(|(_, w)| w).sum::<f64>())
            .collect();
        let damping = 0.85;
        let mut rank = vec![1.0 / n as f64; n];
        for _ in 0..100 {
            let dangling: f64 = (0..n).filter(|&i| out_sum[i] == 0.0).map(|i| rank[i]).sum();
            let mut next = vec![(1.0 - damping) / n as f64 + damping * dangling / n as f64; n];
            for (i, outs) in out.iter().enumerate() {
                if out_sum[i] == 0.0 {
                    continue;
                }
                for &(j, w) in outs {
                    next[j] += damping * rank[i] * w / out_sum[i];
                }
            }
            let delta: f64 = rank.iter().zip(&next).map(|(a, b)| (a - b).abs()).sum();
            rank = next;
            if delta < 1e-6 {
                break;
            }
        }
        rank
    }

    /// Union-find component id per node, over the undirected projection.
    pub fn components(&self) -> Vec<usize> {
        let n = self.nodes.len();
        let mut parent: Vec<usize> = (0..n).collect();
        for e in &self.edges {
            union(&mut parent, e.src, e.tgt);
        }
        (0..n).map(|i| find(&mut parent, i)).collect()
    }

    /// BFS shortest path over the undirected projection. Each hop carries
    /// the rel that connected it (first node has `None`).
    pub fn shortest_path(
        &self,
        from: &NodeKey,
        to: &NodeKey,
    ) -> Option<Vec<(NodeKey, Option<String>)>> {
        let &s = self.index.get(from)?;
        let &t = self.index.get(to)?;
        let n = self.nodes.len();
        let mut prev: Vec<Option<(usize, String)>> = vec![None; n];
        let mut visited = vec![false; n];
        visited[s] = true;
        let mut queue = VecDeque::from([s]);
        while let Some(u) = queue.pop_front() {
            if u == t {
                break;
            }
            for (v, _, rel) in &self.undirected[u] {
                if !visited[*v] {
                    visited[*v] = true;
                    prev[*v] = Some((u, rel.clone()));
                    queue.push_back(*v);
                }
            }
        }
        if !visited[t] {
            return None;
        }
        let mut path = vec![(self.nodes[s].clone(), None)];
        let mut tail = Vec::new();
        let mut cur = t;
        while cur != s {
            let (p, rel) = prev[cur].clone().unwrap();
            tail.push((self.nodes[cur].clone(), Some(rel)));
            cur = p;
        }
        tail.reverse();
        path.extend(tail);
        Some(path)
    }

    /// BFS to `depth`, optionally restricted to one rel, with hop distance.
    pub fn neighbors(&self, node: &NodeKey, depth: u32, rel: Option<&str>) -> Vec<(NodeKey, u32)> {
        let Some(&s) = self.index.get(node) else {
            return Vec::new();
        };
        let n = self.nodes.len();
        let mut dist = vec![u32::MAX; n];
        dist[s] = 0;
        let mut queue = VecDeque::from([s]);
        let mut out = Vec::new();
        while let Some(u) = queue.pop_front() {
            let d = dist[u];
            if d >= depth {
                continue;
            }
            for (v, _, r) in &self.undirected[u] {
                if let Some(filter) = rel {
                    if r != filter {
                        continue;
                    }
                }
                if dist[*v] == u32::MAX {
                    dist[*v] = d + 1;
                    out.push((self.nodes[*v].clone(), d + 1));
                    queue.push_back(*v);
                }
            }
        }
        out
    }

    /// Directed BFS over edges of a single rel, following edge direction.
    /// Returns `(node, depth)` in BFS order, excluding the start node.
    /// Cycle-safe: each node is visited at most once.
    pub fn transitive_reachable(&self, start: &NodeKey, rel: &str) -> Vec<(NodeKey, u32)> {
        let Some(&s) = self.index.get(start) else {
            return Vec::new();
        };
        let n = self.nodes.len();
        let mut out_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for e in &self.edges {
            if e.rel == rel {
                out_adj[e.src].push(e.tgt);
            }
        }
        let mut dist = vec![u32::MAX; n];
        dist[s] = 0;
        let mut queue = VecDeque::from([s]);
        let mut out = Vec::new();
        while let Some(u) = queue.pop_front() {
            let d = dist[u];
            for &v in &out_adj[u] {
                if dist[v] == u32::MAX {
                    dist[v] = d + 1;
                    out.push((self.nodes[v].clone(), d + 1));
                    queue.push_back(v);
                }
            }
        }
        out
    }

    /// Directed BFS over edges of a single rel, tracking the parent each
    /// node was discovered from. `reverse` walks edges backwards (tgt → src).
    /// Returns `(node, depth, parent)` in BFS order, excluding the start
    /// node. Cycle-safe: each node is visited at most once.
    pub fn transitive_reachable_traced(
        &self,
        start: &NodeKey,
        rel: &str,
        reverse: bool,
    ) -> Vec<(NodeKey, u32, NodeKey)> {
        let Some(&s) = self.index.get(start) else {
            return Vec::new();
        };
        let n = self.nodes.len();
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for e in &self.edges {
            if e.rel == rel {
                if reverse {
                    adj[e.tgt].push(e.src);
                } else {
                    adj[e.src].push(e.tgt);
                }
            }
        }
        let mut dist = vec![u32::MAX; n];
        dist[s] = 0;
        let mut queue = VecDeque::from([s]);
        let mut out = Vec::new();
        while let Some(u) = queue.pop_front() {
            let d = dist[u];
            for &v in &adj[u] {
                if dist[v] == u32::MAX {
                    dist[v] = d + 1;
                    out.push((self.nodes[v].clone(), d + 1, self.nodes[u].clone()));
                    queue.push_back(v);
                }
            }
        }
        out
    }

    /// Directed cycles over the subgraph restricted to `rels` (e.g. the
    /// canonical transitive rels). Each cycle is reported once as a closed
    /// node path (first node repeated at the end). Self-loops are reported
    /// as `[n, n]`.
    pub fn cycles(&self, rels: &[&str]) -> Vec<Vec<NodeKey>> {
        let n = self.nodes.len();
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for e in &self.edges {
            if rels.contains(&e.rel.as_str()) {
                adj[e.src].push(e.tgt);
            }
        }
        let mut color = vec![0u8; n]; // 0 = unvisited, 1 = on stack, 2 = done
        let mut stack: Vec<usize> = Vec::new();
        let mut seen: HashSet<Vec<usize>> = HashSet::new();
        let mut found: Vec<Vec<usize>> = Vec::new();
        for s in 0..n {
            if color[s] == 0 {
                cycle_dfs(s, &adj, &mut color, &mut stack, &mut seen, &mut found);
            }
        }
        found
            .iter()
            .map(|cyc| {
                let mut path: Vec<NodeKey> = cyc.iter().map(|&i| self.nodes[i].clone()).collect();
                path.push(self.nodes[cyc[0]].clone());
                path
            })
            .collect()
    }

    /// Nodes with weighted degree ≤ 1.
    pub fn orphans(&self) -> Vec<NodeKey> {
        self.degree()
            .iter()
            .enumerate()
            .filter(|(_, d)| **d <= 1.0)
            .map(|(i, _)| self.nodes[i].clone())
            .collect()
    }

    /// Simple-graph density: unique connected pairs / (N*(N-1)/2).
    pub fn density(&self) -> f64 {
        let n = self.nodes.len();
        if n < 2 {
            return 0.0;
        }
        let mut pairs: HashSet<(usize, usize)> = HashSet::new();
        for e in &self.edges {
            if e.src != e.tgt {
                pairs.insert((e.src.min(e.tgt), e.src.max(e.tgt)));
            }
        }
        pairs.len() as f64 / (n as f64 * (n as f64 - 1.0) / 2.0)
    }
}

/// Gray-stack DFS: a back edge to an on-stack node yields one cycle, the
/// stack slice from that node to the top. Cycles are deduped by their
/// rotation-normalized node sequence.
fn cycle_dfs(
    u: usize,
    adj: &[Vec<usize>],
    color: &mut [u8],
    stack: &mut Vec<usize>,
    seen: &mut HashSet<Vec<usize>>,
    found: &mut Vec<Vec<usize>>,
) {
    color[u] = 1;
    stack.push(u);
    for &v in &adj[u] {
        match color[v] {
            0 => cycle_dfs(v, adj, color, stack, seen, found),
            1 => {
                let pos = stack.iter().position(|&x| x == v).unwrap();
                let cyc = stack[pos..].to_vec();
                let minpos = cyc
                    .iter()
                    .enumerate()
                    .min_by_key(|&(_, &x)| x)
                    .map(|(i, _)| i)
                    .unwrap();
                let mut norm: Vec<usize> = cyc[minpos..].to_vec();
                norm.extend_from_slice(&cyc[..minpos]);
                if seen.insert(norm) {
                    found.push(cyc);
                }
            }
            _ => {}
        }
    }
    stack.pop();
    color[u] = 2;
}

fn find(parent: &mut [usize], x: usize) -> usize {
    let mut root = x;
    while parent[root] != root {
        root = parent[root];
    }
    let mut cur = x;
    while parent[cur] != root {
        let next = parent[cur];
        parent[cur] = root;
        cur = next;
    }
    root
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra] = rb;
    }
}

/// min/max/mean/median over a degree vector.
pub fn degree_stats(deg: &[f64]) -> Value {
    if deg.is_empty() {
        return json!({"min": 0.0, "max": 0.0, "mean": 0.0, "median": 0.0});
    }
    let mut sorted = deg.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let len = sorted.len();
    let mean = sorted.iter().sum::<f64>() / len as f64;
    let median = if len % 2 == 1 {
        sorted[len / 2]
    } else {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
    };
    json!({
        "min": sorted[0],
        "max": sorted[len - 1],
        "mean": mean,
        "median": median,
    })
}

/// Compact summary for prime/doctor: counts + top-3 PageRank nodes.
pub fn summary(conn: &Connection) -> Result<Value> {
    let g = load(conn)?;
    let ranks = g.pagerank();
    let mut order: Vec<usize> = (0..g.nodes.len()).collect();
    order.sort_by(|a, b| {
        ranks[*b]
            .partial_cmp(&ranks[*a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top_central: Vec<Value> = order
        .iter()
        .take(3)
        .map(|&i| json!({"node": fmt_node(&g.nodes[i]), "score": ranks[i]}))
        .collect();
    Ok(json!({
        "nodes": g.nodes.len(),
        "edges": g.edges.len(),
        "density": g.density(),
        "orphans": g.orphans().len(),
        "top_central": top_central,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-build a graph over decision nodes `n` with directed edges
    /// `(src, tgt, rel)`.
    fn chain_graph(n: usize, edges: &[(usize, usize, &str)]) -> Graph {
        let nodes: Vec<NodeKey> = (1..=n)
            .map(|i| ("decision".to_string(), i.to_string()))
            .collect();
        let index: HashMap<NodeKey, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), i))
            .collect();
        let mut undirected = vec![Vec::new(); n];
        let mut es = Vec::new();
        for &(s, t, rel) in edges {
            undirected[s].push((t, 1.0, rel.to_string()));
            if s != t {
                undirected[t].push((s, 1.0, rel.to_string()));
            }
            es.push(Edge {
                src: s,
                tgt: t,
                rel: rel.to_string(),
                weight: 1.0,
                symmetric: false,
                origin: "manual".to_string(),
            });
        }
        Graph {
            nodes,
            index,
            edges: es,
            undirected,
        }
    }

    #[test]
    fn transitive_reachable_follows_direction_with_depth() {
        // 1 -supersedes-> 2 -supersedes-> 3
        let g = chain_graph(3, &[(0, 1, "supersedes"), (1, 2, "supersedes")]);
        let r = g.transitive_reachable(&("decision".into(), "1".into()), "supersedes");
        assert_eq!(
            r,
            vec![
                (("decision".to_string(), "2".to_string()), 1),
                (("decision".to_string(), "3".to_string()), 2),
            ]
        );
        // Against direction: nothing reachable.
        assert!(g
            .transitive_reachable(&("decision".into(), "3".into()), "supersedes")
            .is_empty());
        // Other rels are ignored.
        assert!(g
            .transitive_reachable(&("decision".into(), "1".into()), "depends_on")
            .is_empty());
    }

    #[test]
    fn transitive_reachable_terminates_on_cycles() {
        let g = chain_graph(
            3,
            &[
                (0, 1, "depends_on"),
                (1, 2, "depends_on"),
                (2, 0, "depends_on"),
            ],
        );
        let mut r = g.transitive_reachable(&("decision".into(), "1".into()), "depends_on");
        r.sort();
        assert_eq!(r.len(), 2); // nodes 2 and 3, each once
    }

    #[test]
    fn cycles_finds_cycle_once_with_closed_path() {
        let g = chain_graph(
            3,
            &[
                (0, 1, "depends_on"),
                (1, 2, "depends_on"),
                (2, 0, "depends_on"),
            ],
        );
        let cyc = g.cycles(&["depends_on", "supersedes"]);
        assert_eq!(cyc.len(), 1);
        let path: Vec<String> = cyc[0].iter().map(fmt_node).collect();
        assert_eq!(path.first(), path.last());
        assert_eq!(path.len(), 4);
        // A DAG has no cycles.
        let dag = chain_graph(3, &[(0, 1, "supersedes"), (1, 2, "supersedes")]);
        assert!(dag.cycles(&["supersedes"]).is_empty());
    }

    #[test]
    fn cycles_reports_self_loop() {
        let g = chain_graph(1, &[(0, 0, "depends_on")]);
        let cyc = g.cycles(&["depends_on"]);
        assert_eq!(cyc.len(), 1);
        assert_eq!(cyc[0].len(), 2);
    }
}
