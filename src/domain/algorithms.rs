use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::domain::metrics::{CycleBreakEdge, DetailedFileEdge, MisplacedFile};
use crate::domain::model::FileGraph;

/// Compute propagation cost (`0.0..=1.0`), Lakos ACD, and Lakos NCCD.
///
/// For `n` nodes, performs BFS from each node to measure exact transitive reachability.
pub fn compute_propagation_and_nccd(n: usize, edges: &[(usize, usize)]) -> (f64, f64, f64) {
    if n == 0 {
        return (0.0, 0.0, 0.0);
    }

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(u, v) in edges {
        if u < n && v < n && u != v {
            adj[u].push(v);
        }
    }

    let mut ccd = 0usize;
    let mut visited = vec![0usize; n];
    let mut queue = VecDeque::with_capacity(n);

    for start in 0..n {
        let stamp = start + 1;
        visited[start] = stamp;
        queue.push_back(start);
        let mut reachable = 1usize;

        while let Some(cur) = queue.pop_front() {
            for &next in &adj[cur] {
                if visited[next] != stamp {
                    visited[next] = stamp;
                    reachable += 1;
                    queue.push_back(next);
                }
            }
        }
        ccd += reachable;
    }

    let propagation_cost = round4(ccd as f64 / (n * n) as f64);
    let acd = round3(ccd as f64 / n as f64);
    let balanced_ccd = balanced_binary_tree_ccd(n);
    let nccd = if balanced_ccd > 0.0 {
        round3(ccd as f64 / balanced_ccd)
    } else {
        1.0
    };

    (propagation_cost, acd, nccd)
}

/// Cumulative Component Dependency of a balanced binary tree with `n` nodes (Lakos reference).
fn balanced_binary_tree_ccd(n: usize) -> f64 {
    if n <= 1 {
        return n as f64;
    }
    let mut remaining = n;
    let mut level = 0u32;
    let mut ccd = 0.0_f64;
    while remaining > 0 {
        let nodes_at_level = (1usize << level.min(30)).min(remaining);
        // Each node at depth `level` has `level + 1` nodes on its path to the root (including self).
        ccd += (nodes_at_level as f64) * f64::from(level + 1);
        remaining -= nodes_at_level;
        level += 1;
    }
    ccd
}

/// Compute a minimal-weight set of edges whose removal breaks every cycle inside an SCC.
///
/// Prefers removing edges with the smallest file-edge weight (and non-DIP penalty tie-breaking)
/// while ensuring the returned set is minimal (no redundant removals).
pub fn compute_cycle_break_edges(
    scc_members: &[usize],
    node_ids: &[String],
    agg_edges: &BTreeMap<(usize, usize), Vec<DetailedFileEdge>>,
) -> Vec<CycleBreakEdge> {
    if scc_members.len() < 2 {
        return Vec::new();
    }

    let local_idx: HashMap<usize, usize> = scc_members
        .iter()
        .enumerate()
        .map(|(li, &global)| (global, li))
        .collect();
    let k = scc_members.len();

    let mut internal: Vec<(usize, usize, usize, usize)> = Vec::new();
    for &(u, v) in agg_edges.keys() {
        if let (Some(&lu), Some(&lv)) = (local_idx.get(&u), local_idx.get(&v))
            && lu != lv
        {
            internal.push((lu, lv, u, v));
        }
    }

    // Sort candidate edges from cheapest to cut -> most expensive to cut.
    let mut active: Vec<bool> = vec![true; internal.len()];
    let mut removed_indices: Vec<usize> = Vec::new();

    // Iteratively remove the cheapest edge that still lies inside a non-trivial SCC.
    loop {
        let scc_of = local_scc_map(k, &internal, &active);
        let mut scc_sizes = vec![0usize; k];
        for &cid in &scc_of {
            scc_sizes[cid] += 1;
        }

        let mut best_edge: Option<(usize, usize)> = None;
        for (ei, &(lu, lv, gu, gv)) in internal.iter().enumerate() {
            if !active[ei] {
                continue;
            }
            if scc_of[lu] == scc_of[lv] && scc_sizes[scc_of[lu]] >= 2 {
                let weight = agg_edges.get(&(gu, gv)).map_or(1, Vec::len);
                let replace = match best_edge {
                    None => true,
                    Some((_, best_w)) => weight < best_w,
                };
                if replace {
                    best_edge = Some((ei, weight));
                }
            }
        }

        match best_edge {
            Some((ei, _)) => {
                active[ei] = false;
                removed_indices.push(ei);
            }
            None => break,
        }
    }

    // Prune any removed edge that is no longer needed (keep set minimal, checking heaviest first).
    removed_indices.sort_by(|&a, &b| {
        let wa = agg_edges
            .get(&(internal[a].2, internal[a].3))
            .map_or(1, Vec::len);
        let wb = agg_edges
            .get(&(internal[b].2, internal[b].3))
            .map_or(1, Vec::len);
        wb.cmp(&wa)
    });

    let mut minimal_removed = Vec::new();
    for ei in removed_indices {
        active[ei] = true;
        if is_local_dag(k, &internal, &active) {
            // Re-adding did not reintroduce a cycle — this cut was redundant.
            continue;
        }
        active[ei] = false;
        minimal_removed.push(ei);
    }

    let mut suggestions: Vec<CycleBreakEdge> = minimal_removed
        .into_iter()
        .map(|ei| {
            let (_, _, gu, gv) = internal[ei];
            let details = agg_edges.get(&(gu, gv)).cloned().unwrap_or_default();
            CycleBreakEdge {
                from: node_ids[gu].clone(),
                to: node_ids[gv].clone(),
                weight: details.len(),
                file_edges: details.into_iter().take(10).collect(),
            }
        })
        .collect();

    suggestions.sort_by(|a, b| {
        a.weight
            .cmp(&b.weight)
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.to.cmp(&b.to))
    });
    suggestions
}

fn local_scc_map(k: usize, internal: &[(usize, usize, usize, usize)], active: &[bool]) -> Vec<usize> {
    let mut g = DiGraph::<usize, ()>::with_capacity(k, internal.len());
    let nodes: Vec<NodeIndex> = (0..k).map(|i| g.add_node(i)).collect();
    for (ei, &(u, v, _, _)) in internal.iter().enumerate() {
        if active[ei] {
            g.add_edge(nodes[u], nodes[v], ());
        }
    }
    let sccs = tarjan_scc(&g);
    let mut map = vec![0usize; k];
    for (cid, comp) in sccs.iter().enumerate() {
        for &n in comp {
            map[n.index()] = cid;
        }
    }
    map
}

fn is_local_dag(k: usize, internal: &[(usize, usize, usize, usize)], active: &[bool]) -> bool {
    let mut g = DiGraph::<usize, ()>::with_capacity(k, internal.len());
    let nodes: Vec<NodeIndex> = (0..k).map(|i| g.add_node(i)).collect();
    for (ei, &(u, v, _, _)) in internal.iter().enumerate() {
        if active[ei] {
            g.add_edge(nodes[u], nodes[v], ());
        }
    }
    tarjan_scc(&g).iter().all(|c| c.len() == 1)
}

/// Run Louvain modularity clustering on the undirected file coupling graph and detect
/// files whose actual coupling belongs much more strongly to a different architectural module.
pub fn detect_louvain_and_misplaced(
    graph: &FileGraph,
) -> (HashMap<String, usize>, Vec<MisplacedFile>) {
    let n = graph.files.len();
    if n == 0 {
        return (HashMap::new(), Vec::new());
    }

    let path_to_idx: HashMap<&str, usize> = graph
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path.as_str(), i))
        .collect();

    // Build undirected weighted adjacency list (weight = 1 + symbol count factor).
    let mut adj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); n];
    let mut deg = vec![0.0_f64; n];
    let mut total_w = 0.0_f64;

    for e in &graph.edges {
        let (Some(&u), Some(&v)) = (
            path_to_idx.get(e.from.as_str()),
            path_to_idx.get(e.to.as_str()),
        ) else {
            continue;
        };
        if u == v {
            continue;
        }
        let w = 1.0 + (e.symbols.len().min(6) as f64) * 0.2;
        *adj[u].entry(v).or_insert(0.0) += w;
        *adj[v].entry(u).or_insert(0.0) += w;
        deg[u] += w;
        deg[v] += w;
        total_w += w;
    }

    let two_m = 2.0 * total_w;
    let mut comm: Vec<usize> = (0..n).collect();
    let mut sigma_tot: Vec<f64> = deg.clone();

    if two_m > 0.0 {
        for _pass in 0..18 {
            let mut moved = false;
            for i in 0..n {
                if adj[i].is_empty() {
                    continue;
                }
                let cur_c = comm[i];
                let ki = deg[i];
                sigma_tot[cur_c] -= ki;

                let mut neigh_weights: HashMap<usize, f64> = HashMap::new();
                for (&nb, &w) in &adj[i] {
                    *neigh_weights.entry(comm[nb]).or_insert(0.0) += w;
                }

                let mut best_c = cur_c;
                let mut best_gain = 0.0_f64;
                let k_i_in_cur = neigh_weights.get(&cur_c).copied().unwrap_or(0.0);
                let cur_score = k_i_in_cur - (sigma_tot[cur_c] * ki) / two_m;

                // Deterministic tie-breaking by community ID.
                let mut candidates: Vec<(usize, f64)> = neigh_weights.into_iter().collect();
                candidates.sort_by_key(|&(c, _)| c);

                for (cand_c, k_i_in) in candidates {
                    if cand_c == cur_c {
                        continue;
                    }
                    let gain = (k_i_in - (sigma_tot[cand_c] * ki) / two_m) - cur_score;
                    if gain > best_gain + 1e-9 {
                        best_gain = gain;
                        best_c = cand_c;
                    }
                }

                comm[i] = best_c;
                sigma_tot[best_c] += ki;
                if best_c != cur_c {
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
    }

    // Renumber communities deterministically by size (largest cluster = 0).
    let mut comm_sizes: HashMap<usize, usize> = HashMap::new();
    for &c in &comm {
        *comm_sizes.entry(c).or_insert(0) += 1;
    }
    let mut ranked_comms: Vec<(usize, usize)> = comm_sizes.into_iter().collect();
    ranked_comms.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let remap: HashMap<usize, usize> = ranked_comms
        .into_iter()
        .enumerate()
        .map(|(new_id, (old_id, _))| (old_id, new_id))
        .collect();

    let mut file_cluster = HashMap::with_capacity(n);
    for (i, f) in graph.files.iter().enumerate() {
        file_cluster.insert(f.path.clone(), remap[&comm[i]]);
    }

    // Identify files exhibiting module-level Feature Envy:
    // strong OUTGOING coupling to a single foreign module while NOT being a multi-consumer shared core.
    let mut misplaced = Vec::new();
    let mut module_file_counts: HashMap<&str, usize> = HashMap::new();
    for f in &graph.files {
        *module_file_counts.entry(f.module_id.as_str()).or_insert(0) += 1;
    }

    let mut outgoing_targets: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    let mut incoming_modules: Vec<HashSet<&str>> = vec![HashSet::new(); n];
    for e in &graph.edges {
        if let (Some(&u), Some(&v)) = (
            path_to_idx.get(e.from.as_str()),
            path_to_idx.get(e.to.as_str()),
        ) && u != v
        {
            outgoing_targets[u].insert(v);
            let from_mod = graph.files[u].module_id.as_str();
            let to_mod = graph.files[v].module_id.as_str();
            if from_mod != to_mod {
                incoming_modules[v].insert(from_mod);
            }
        }
    }

    for (i, file) in graph.files.iter().enumerate() {
        let own_mod = file.module_id.as_str();
        // Skip single-file entrypoint/utility modules and multi-consumer shared cores.
        if module_file_counts.get(own_mod).copied().unwrap_or(0) <= 2
            || incoming_modules[i].len() >= 2
        {
            continue;
        }
        let mut by_mod: HashMap<&str, usize> = HashMap::new();
        for &nb in &outgoing_targets[i] {
            *by_mod
                .entry(graph.files[nb].module_id.as_str())
                .or_insert(0) += 1;
        }
        let internal = by_mod.get(own_mod).copied().unwrap_or(0);
        let total: usize = by_mod.values().sum();
        if total < 3 {
            continue;
        }

        if let Some((&top_mod, &top_cnt)) = by_mod
            .iter()
            .filter(|(m, _)| **m != own_mod)
            .max_by(|(ma, ca), (mb, cb)| ca.cmp(cb).then_with(|| mb.cmp(ma)))
            && top_cnt >= 3
            && top_cnt >= internal.saturating_mul(2).max(3)
        {
            let affinity = round4(top_cnt as f64 / total as f64);
            if affinity >= 0.60 {
                misplaced.push(MisplacedFile {
                    file: file.path.clone(),
                    current_module: own_mod.to_string(),
                    suggested_module: top_mod.to_string(),
                    internal_edges: internal,
                    suggested_module_edges: top_cnt,
                    affinity,
                });
            }
        }
    }

    misplaced.sort_by(|a, b| {
        b.suggested_module_edges
            .cmp(&a.suggested_module_edges)
            .then_with(|| b.affinity.total_cmp(&a.affinity))
            .then_with(|| a.file.cmp(&b.file))
    });
    misplaced.truncate(30);

    (file_cluster, misplaced)
}

fn round3(v: f64) -> f64 {
    (v * 1_000.0).round() / 1_000.0
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
