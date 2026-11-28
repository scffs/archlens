use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::domain::algorithms::{compute_cycle_break_edges, compute_propagation_and_nccd};
use crate::domain::metrics::{
    ArchitectureView, ArchitectureZone, CycleGroup, DetailedFileEdge, NodeMetrics, RuleViolation,
    SdpViolation, ViewEdge, ViewSummary, round3, round4,
};
use crate::domain::model::{FileGraph, Language, SymbolCounts};
use crate::domain::projection::ViewProjection;

const MAX_SAMPLE_FILE_EDGES: usize = 12;
const MAX_DETAILED_FILE_EDGES: usize = 40;

/// Compute Clean Architecture coupling, instability, abstractness, Main Sequence distance,
/// Propagation Cost, ACD/NCCD, Tarjan SCC cycles + break suggestions, SDP violations, and DSM order.
pub fn analyze_view<S: std::hash::BuildHasher>(
    graph: &FileGraph,
    projection: &ViewProjection,
    main_sequence_band: f64,
    sdp_tolerance: f64,
    file_clusters: &HashMap<String, usize, S>,
    rule_violations: &[RuleViolation],
) -> ArchitectureView {
    let mut group_files: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, file) in graph.files.iter().enumerate() {
        let group_id = projection
            .file_to_group
            .get(&file.path)
            .cloned()
            .unwrap_or_else(|| file.path.clone());
        group_files.entry(group_id).or_default().push(idx);
    }

    let node_ids: Vec<String> = group_files.keys().cloned().collect();
    let id_to_idx: HashMap<&str, usize> = node_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let violated_file_pairs: HashSet<(&str, &str)> = rule_violations
        .iter()
        .map(|rv| (rv.from_file.as_str(), rv.to_file.as_str()))
        .collect();

    let n = node_ids.len();
    let mut internal_edges = vec![0usize; n];
    let mut external_dependents: Vec<HashSet<&str>> = vec![HashSet::new(); n];
    let mut external_dependencies: Vec<HashSet<&str>> = vec![HashSet::new(); n];
    let mut comp_dependents: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    let mut comp_dependencies: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    let mut incident_total = vec![0usize; n];
    let mut incident_dip = vec![0usize; n];
    let mut agg_edges: BTreeMap<(usize, usize), Vec<DetailedFileEdge>> = BTreeMap::new();

    for edge in &graph.edges {
        let Some(from_group) = projection.file_to_group.get(&edge.from) else {
            continue;
        };
        let Some(to_group) = projection.file_to_group.get(&edge.to) else {
            continue;
        };
        let Some(&u) = id_to_idx.get(from_group.as_str()) else {
            continue;
        };
        let Some(&v) = id_to_idx.get(to_group.as_str()) else {
            continue;
        };

        if u == v {
            internal_edges[u] += 1;
        } else {
            external_dependents[v].insert(edge.from.as_str());
            external_dependencies[u].insert(edge.to.as_str());
            comp_dependents[v].insert(u);
            comp_dependencies[u].insert(v);
            incident_total[u] += 1;
            incident_total[v] += 1;
            if edge.is_dip {
                incident_dip[u] += 1;
                incident_dip[v] += 1;
            }
            agg_edges.entry((u, v)).or_default().push(DetailedFileEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
                line: edge.line,
                lines: edge.lines.clone(),
                symbols: edge.symbols.clone(),
                kind: edge.kind,
                is_dip: edge.is_dip,
            });
        }
    }

    // Build petgraph DiGraph for SCC cycle detection and DSM condensation ordering.
    let mut di = DiGraph::<usize, ()>::with_capacity(n, agg_edges.len());
    let pg_nodes: Vec<NodeIndex> = (0..n).map(|i| di.add_node(i)).collect();
    let edge_pairs: Vec<(usize, usize)> = agg_edges.keys().copied().collect();
    for &(u, v) in &edge_pairs {
        di.add_edge(pg_nodes[u], pg_nodes[v], ());
    }

    let (propagation_cost, acd, nccd) = compute_propagation_and_nccd(n, &edge_pairs);

    let sccs = tarjan_scc(&di);
    let mut node_scc: Vec<usize> = vec![0; n];
    let mut in_cycle = vec![false; n];
    let mut cycle_scc_ids = Vec::new();

    for (scc_idx, comp) in sccs.iter().enumerate() {
        for &pg_idx in comp {
            node_scc[pg_idx.index()] = scc_idx;
        }
        if comp.len() >= 2 {
            cycle_scc_ids.push(scc_idx);
            for &pg_idx in comp {
                in_cycle[pg_idx.index()] = true;
            }
        }
    }

    // Compute per-node metrics.
    let mut nodes = Vec::with_capacity(n);
    for (i, id) in node_ids.iter().enumerate() {
        let file_indices = &group_files[id];
        let mut loc = 0usize;
        let mut symbols = SymbolCounts::default();
        let mut languages: BTreeMap<Language, usize> = BTreeMap::new();
        let mut ext_deps: BTreeSet<&str> = BTreeSet::new();
        let mut file_paths = Vec::with_capacity(file_indices.len());
        let mut commits = 0usize;
        let mut churn = 0usize;
        let mut max_hotspot = 0.0_f64;
        let mut author_weight: HashMap<&str, f64> = HashMap::new();
        let mut cluster_votes: HashMap<usize, usize> = HashMap::new();

        for &fi in file_indices {
            let f = &graph.files[fi];
            loc += f.loc;
            symbols.add_assign(f.symbols);
            *languages.entry(f.language).or_insert(0) += 1;
            for ext in &f.external_imports {
                ext_deps.insert(ext.as_str());
            }
            if let Some(ref g) = f.git {
                commits += g.commits;
                churn += g.lines_added + g.lines_deleted;
                if g.hotspot_score > max_hotspot {
                    max_hotspot = g.hotspot_score;
                }
                if !g.top_author.is_empty() {
                    let weighted = ((g.commits as f64) * g.top_author_share).max(1.0);
                    *author_weight.entry(g.top_author.as_str()).or_insert(0.0) += weighted;
                }
            }
            if let Some(&cid) = file_clusters.get(&f.path) {
                *cluster_votes.entry(cid).or_insert(0) += 1;
            }
            file_paths.push(f.path.clone());
        }
        file_paths.sort();

        let (top_author, top_author_share) = if author_weight.is_empty() {
            (String::new(), 0.0)
        } else {
            let total_w: f64 = author_weight.values().sum();
            let (name, cnt) = author_weight
                .into_iter()
                .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(a.0)))
                .unwrap_or(("", 0.0));
            (
                name.to_string(),
                if total_w > 0.0 {
                    round4(cnt / total_w)
                } else {
                    0.0
                },
            )
        };

        let louvain_cluster = cluster_votes
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map_or(0, |(cid, _)| cid);

        let file_count = file_indices.len();
        let ca = external_dependents[i].len();
        let ce = external_dependencies[i].len();
        let cohesion = if file_count > 0 {
            round3((internal_edges[i] + 1) as f64 / file_count as f64)
        } else {
            0.0
        };

        let instability = if ca + ce == 0 {
            None
        } else {
            Some(round4(ce as f64 / (ca + ce) as f64))
        };

        let abstractness = if symbols.total() == 0 {
            None
        } else {
            Some(round4(
                symbols.abstract_declarations as f64 / symbols.total() as f64,
            ))
        };

        let distance = match (instability, abstractness) {
            (Some(inst), Some(abstr)) => Some(round4((abstr + inst - 1.0).abs())),
            _ => None,
        };

        let zone = classify_zone(instability, abstractness, distance, main_sequence_band);
        let active_pain = zone == ArchitectureZone::ZoneOfPain && max_hotspot >= 0.30;
        let primary_language = detect_primary_language(&languages);
        let dip_ratio = if incident_total[i] > 0 {
            round4(incident_dip[i] as f64 / incident_total[i] as f64)
        } else {
            0.0
        };

        nodes.push(NodeMetrics {
            id: id.clone(),
            label: id.clone(),
            file_count,
            loc,
            primary_language,
            languages,
            files: file_paths,
            symbols,
            afferent_coupling: ca,
            efferent_coupling: ce,
            component_fan_in: comp_dependents[i].len(),
            component_fan_out: comp_dependencies[i].len(),
            external_dep_count: ext_deps.len(),
            internal_edges: internal_edges[i],
            cohesion,
            instability,
            abstractness,
            distance,
            zone,
            in_cycle: in_cycle[i],
            dip_ratio,
            commits,
            churn,
            hotspot_score: round4(max_hotspot),
            top_author_share,
            top_author,
            active_pain,
            louvain_cluster,
        });
    }

    // Build aggregated view edges and SDP violations.
    let mut edges = Vec::with_capacity(agg_edges.len());
    let mut sdp_violations = Vec::new();
    let mut view_rule_violation_edges = 0usize;

    for ((u, v), details) in &agg_edges {
        let is_cycle_edge = in_cycle[*u] && node_scc[*u] == node_scc[*v];
        let dip_weight = details.iter().filter(|d| d.is_dip).count();
        let is_dip_only = !details.is_empty() && dip_weight == details.len();
        let is_rule_violation = details
            .iter()
            .any(|d| violated_file_pairs.contains(&(d.from.as_str(), d.to.as_str())));
        if is_rule_violation {
            view_rule_violation_edges += 1;
        }

        let mut is_sdp_violation = false;
        if let (Some(i_from), Some(i_to)) = (nodes[*u].instability, nodes[*v].instability) {
            let delta = i_to - i_from;
            if delta > sdp_tolerance {
                is_sdp_violation = true;
                sdp_violations.push(SdpViolation {
                    from: node_ids[*u].clone(),
                    to: node_ids[*v].clone(),
                    from_instability: i_from,
                    to_instability: i_to,
                    delta: round4(delta),
                    weight: details.len(),
                    is_dip_only,
                });
            }
        }

        let mut sorted_details = details.clone();
        sorted_details.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));

        let sample_file_edges: Vec<(String, String)> = sorted_details
            .iter()
            .take(MAX_SAMPLE_FILE_EDGES)
            .map(|d| (d.from.clone(), d.to.clone()))
            .collect();
        sorted_details.truncate(MAX_DETAILED_FILE_EDGES);

        edges.push(ViewEdge {
            from: node_ids[*u].clone(),
            to: node_ids[*v].clone(),
            weight: details.len(),
            file_edges: sorted_details,
            sample_file_edges,
            is_cycle_edge,
            is_sdp_violation,
            dip_weight,
            is_rule_violation,
        });
    }

    sdp_violations.sort_by(|a, b| {
        a.is_dip_only
            .cmp(&b.is_dip_only)
            .then_with(|| b.delta.total_cmp(&a.delta))
            .then_with(|| b.weight.cmp(&a.weight))
            .then_with(|| a.from.cmp(&b.from))
    });

    // Build CycleGroups with minimum feedback arc set break suggestions.
    let mut cycles = Vec::with_capacity(cycle_scc_ids.len());
    for scc_idx in cycle_scc_ids {
        let member_indices: Vec<usize> = sccs[scc_idx].iter().map(|pg| pg.index()).collect();
        let mut members: Vec<String> = member_indices
            .iter()
            .map(|&idx| node_ids[idx].clone())
            .collect();
        members.sort();

        let mut edge_count = 0usize;
        let mut sample_file_edges = Vec::new();
        for ((u, v), details) in &agg_edges {
            if node_scc[*u] == scc_idx && node_scc[*v] == scc_idx {
                edge_count += 1;
                if sample_file_edges.len() < MAX_SAMPLE_FILE_EDGES {
                    for d in details {
                        if sample_file_edges.len() >= MAX_SAMPLE_FILE_EDGES {
                            break;
                        }
                        sample_file_edges.push((d.from.clone(), d.to.clone()));
                    }
                }
            }
        }

        let break_suggestions = compute_cycle_break_edges(&member_indices, &node_ids, &agg_edges);

        cycles.push(CycleGroup {
            nodes: members,
            edge_count,
            sample_file_edges,
            break_suggestions,
        });
    }
    cycles.sort_by(|a, b| {
        b.nodes
            .len()
            .cmp(&a.nodes.len())
            .then_with(|| a.nodes.cmp(&b.nodes))
    });

    let dsm_order = compute_dsm_order(&nodes, &sccs, &node_scc, &agg_edges);
    let summary = compute_summary(
        &nodes,
        SummaryInputs {
            edge_count: edges.len(),
            cycle_count: cycles.len(),
            sdp_violation_count: sdp_violations.len(),
            propagation_cost,
            acd,
            nccd,
            rule_violation_count: view_rule_violation_edges,
        },
    );

    ArchitectureView {
        id: projection.id.clone(),
        title: projection.title.clone(),
        description: projection.description.clone(),
        nodes,
        edges,
        cycles,
        sdp_violations,
        dsm_order,
        summary,
    }
}

fn classify_zone(
    instability: Option<f64>,
    abstractness: Option<f64>,
    distance: Option<f64>,
    band: f64,
) -> ArchitectureZone {
    let Some(inst) = instability else {
        return ArchitectureZone::Isolated;
    };
    let Some(abstr) = abstractness else {
        return ArchitectureZone::NoEntities;
    };
    let dist = distance.unwrap_or_else(|| (abstr + inst - 1.0).abs());
    if dist <= band {
        ArchitectureZone::MainSequence
    } else if abstr + inst < 1.0 {
        ArchitectureZone::ZoneOfPain
    } else {
        ArchitectureZone::ZoneOfUselessness
    }
}

fn detect_primary_language(languages: &BTreeMap<Language, usize>) -> String {
    if languages.is_empty() {
        return "Unknown".to_string();
    }
    let total: usize = languages.values().sum();
    let mut ranked: Vec<(Language, usize)> = languages.iter().map(|(&l, &c)| (l, c)).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    if ranked.len() > 1 && ranked[1].1 * 5 >= total {
        "Mixed".to_string()
    } else {
        ranked[0].0.display_name().to_string()
    }
}

#[derive(Clone, Eq, PartialEq)]
struct DsmQueueItem {
    instability_bp: u32,
    inv_fan_in: usize,
    scc_idx: usize,
}

impl Ord for DsmQueueItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .instability_bp
            .cmp(&self.instability_bp)
            .then_with(|| self.inv_fan_in.cmp(&other.inv_fan_in))
            .then_with(|| other.scc_idx.cmp(&self.scc_idx))
    }
}

impl PartialOrd for DsmQueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compute_dsm_order(
    nodes: &[NodeMetrics],
    sccs: &[Vec<NodeIndex>],
    node_scc: &[usize],
    agg_edges: &BTreeMap<(usize, usize), Vec<DetailedFileEdge>>,
) -> Vec<String> {
    let scc_count = sccs.len();
    let mut rev_adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); scc_count];
    let mut indegree = vec![0usize; scc_count];

    for &(u, v) in agg_edges.keys() {
        let su = node_scc[u];
        let sv = node_scc[v];
        if su != sv && rev_adj[sv].insert(su) {
            indegree[su] += 1;
        }
    }

    let scc_priority = |scc_idx: usize| -> DsmQueueItem {
        let members = &sccs[scc_idx];
        let mut inst_sum = 0.0;
        let mut inst_cnt = 0usize;
        let mut fan_in_sum = 0usize;
        for &pg in members {
            let m = &nodes[pg.index()];
            if let Some(i) = m.instability {
                inst_sum += i;
                inst_cnt += 1;
            }
            fan_in_sum += m.afferent_coupling;
        }
        let avg_i = if inst_cnt > 0 {
            inst_sum / inst_cnt as f64
        } else {
            1.1
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let instability_bp = (avg_i * 10_000.0).clamp(0.0, 20_000.0) as u32;
        DsmQueueItem {
            instability_bp,
            inv_fan_in: fan_in_sum,
            scc_idx,
        }
    };

    let mut heap = BinaryHeap::new();
    for (scc_idx, &deg) in indegree.iter().enumerate() {
        if deg == 0 {
            heap.push(scc_priority(scc_idx));
        }
    }

    let mut ordered_sccs = Vec::with_capacity(scc_count);
    while let Some(item) = heap.pop() {
        ordered_sccs.push(item.scc_idx);
        for &next_scc in &rev_adj[item.scc_idx] {
            indegree[next_scc] -= 1;
            if indegree[next_scc] == 0 {
                heap.push(scc_priority(next_scc));
            }
        }
    }

    let mut order = Vec::with_capacity(nodes.len());
    for scc_idx in ordered_sccs {
        let mut members: Vec<usize> = sccs[scc_idx].iter().map(|pg| pg.index()).collect();
        members.sort_by(|&a, &b| {
            let ia = nodes[a].instability.unwrap_or(1.1);
            let ib = nodes[b].instability.unwrap_or(1.1);
            ia.total_cmp(&ib)
                .then_with(|| nodes[b].afferent_coupling.cmp(&nodes[a].afferent_coupling))
                .then_with(|| nodes[a].id.cmp(&nodes[b].id))
        });
        for idx in members {
            order.push(nodes[idx].id.clone());
        }
    }
    order
}

#[derive(Clone, Copy)]
struct SummaryInputs {
    edge_count: usize,
    cycle_count: usize,
    sdp_violation_count: usize,
    propagation_cost: f64,
    acd: f64,
    nccd: f64,
    rule_violation_count: usize,
}

fn compute_summary(nodes: &[NodeMetrics], inputs: SummaryInputs) -> ViewSummary {
    let mut nodes_in_cycles = 0usize;
    let mut zone_of_pain_count = 0usize;
    let mut active_pain_count = 0usize;
    let mut zone_of_uselessness_count = 0usize;
    let mut main_sequence_count = 0usize;
    let mut isolated_count = 0usize;
    let mut no_entities_count = 0usize;

    let mut dist_sum = 0.0;
    let mut dist_cnt = 0usize;
    let mut max_dist: Option<f64> = None;

    let mut inst_sum = 0.0;
    let mut inst_cnt = 0usize;

    let mut abstr_sum = 0.0;
    let mut abstr_cnt = 0usize;

    for node in nodes {
        if node.in_cycle {
            nodes_in_cycles += 1;
        }
        if node.active_pain {
            active_pain_count += 1;
        }
        match node.zone {
            ArchitectureZone::MainSequence => main_sequence_count += 1,
            ArchitectureZone::ZoneOfPain => zone_of_pain_count += 1,
            ArchitectureZone::ZoneOfUselessness => zone_of_uselessness_count += 1,
            ArchitectureZone::Isolated => isolated_count += 1,
            ArchitectureZone::NoEntities => no_entities_count += 1,
        }
        if let Some(d) = node.distance {
            dist_sum += d;
            dist_cnt += 1;
            max_dist = Some(max_dist.map_or(d, |cur| cur.max(d)));
        }
        if let Some(i) = node.instability {
            inst_sum += i;
            inst_cnt += 1;
        }
        if let Some(a) = node.abstractness {
            abstr_sum += a;
            abstr_cnt += 1;
        }
    }

    ViewSummary {
        node_count: nodes.len(),
        edge_count: inputs.edge_count,
        cycle_count: inputs.cycle_count,
        nodes_in_cycles,
        sdp_violation_count: inputs.sdp_violation_count,
        zone_of_pain_count,
        active_pain_count,
        zone_of_uselessness_count,
        main_sequence_count,
        isolated_count,
        no_entities_count,
        average_distance: (dist_cnt > 0).then(|| round4(dist_sum / dist_cnt as f64)),
        max_distance: max_dist.map(round4),
        average_instability: (inst_cnt > 0).then(|| round4(inst_sum / inst_cnt as f64)),
        average_abstractness: (abstr_cnt > 0).then(|| round4(abstr_sum / abstr_cnt as f64)),
        propagation_cost: inputs.propagation_cost,
        acd: inputs.acd,
        nccd: inputs.nccd,
        rule_violation_count: inputs.rule_violation_count,
    }
}
