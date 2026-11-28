#![no_main]

mod support;

use std::collections::BTreeSet;
use std::path::Path;

use archlens::{AnalysisConfig, FileGraph, analyze_file_graph};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let graph = support::file_graph(input);
    let root = Path::new("synthetic");
    let report = analyze_file_graph(root, &graph, &AnalysisConfig::new(root))
        .expect("analyze a valid synthetic graph");
    let files = report.view("files").expect("file view");
    let n = graph.files.len();

    assert_eq!(report.total_files, n);
    assert_eq!(
        report.total_loc,
        graph.files.iter().map(|file| file.loc).sum::<usize>()
    );
    assert_eq!(report.total_internal_edges, graph.edges.len());
    assert_eq!(files.summary.node_count, n);
    assert_eq!(files.summary.edge_count, graph.edges.len());

    let pairs: BTreeSet<_> = graph
        .edges
        .iter()
        .map(|edge| (&edge.from, &edge.to))
        .collect();
    assert_eq!(
        files
            .edges
            .iter()
            .map(|edge| (&edge.from, &edge.to))
            .collect::<BTreeSet<_>>(),
        pairs,
    );
    for view in &report.views {
        let node_ids: BTreeSet<_> = view.nodes.iter().map(|node| &node.id).collect();
        assert_eq!(view.dsm_order.len(), view.nodes.len());
        assert_eq!(view.dsm_order.iter().collect::<BTreeSet<_>>(), node_ids);
        assert!(
            view.edges
                .iter()
                .all(|edge| node_ids.contains(&edge.from) && node_ids.contains(&edge.to))
        );
    }

    let reachable = transitive_closure(&graph);
    let ccd = reachable.iter().flatten().filter(|&&value| value).count();
    let denominator = n.max(1) as f64;
    let expected_acd = ccd as f64 / denominator;
    let tree_ccd: u32 = (1..=n).map(|node| node.ilog2() + 1).sum();
    let expected_nccd = ccd as f64 / f64::from(tree_ccd.max(1));
    // Report values are rounded to three or four decimal places.
    assert!((files.summary.acd - expected_acd).abs() <= 0.000_500_001);
    assert!((files.summary.nccd - expected_nccd).abs() <= 0.000_500_001);
    assert!((files.summary.propagation_cost - expected_acd / denominator).abs() <= 0.000_050_001);

    let expected_cycles: BTreeSet<_> = (0..n)
        .filter_map(|from| {
            let component: BTreeSet<_> = (0..n)
                .filter(|&to| reachable[from][to] && reachable[to][from])
                .map(|to| &graph.files[to].path)
                .collect();
            (component.len() > 1).then_some(component)
        })
        .collect();
    assert_eq!(files.summary.cycle_count, expected_cycles.len());
    assert_eq!(
        files.summary.nodes_in_cycles,
        expected_cycles.iter().map(BTreeSet::len).sum::<usize>()
    );
    assert_eq!(
        files
            .cycles
            .iter()
            .map(|cycle| cycle.nodes.iter().collect())
            .collect::<BTreeSet<_>>(),
        expected_cycles,
    );
});

// The analyzer uses graph traversal. Compute reachability independently on a small matrix.
fn transitive_closure(graph: &FileGraph) -> Vec<Vec<bool>> {
    let n = graph.files.len();
    let mut reachable = vec![vec![false; n]; n];
    for (index, row) in reachable.iter_mut().enumerate() {
        row[index] = true;
    }
    for edge in &graph.edges {
        let from = graph
            .files
            .iter()
            .position(|file| file.path == edge.from)
            .unwrap();
        let to = graph
            .files
            .iter()
            .position(|file| file.path == edge.to)
            .unwrap();
        reachable[from][to] = true;
    }
    for via in 0..n {
        for from in 0..n {
            for to in 0..n {
                reachable[from][to] |= reachable[from][via] && reachable[via][to];
            }
        }
    }
    reachable
}
