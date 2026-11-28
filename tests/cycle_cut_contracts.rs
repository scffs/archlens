use std::collections::BTreeMap;

use archlens::ImportKind;
use archlens::domain::algorithms::compute_cycle_break_edges;
use archlens::domain::metrics::DetailedFileEdge;
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::DiGraph;

#[test]
fn cycle_cut_suggestions_remove_the_shared_edge_and_prune_redundant_cuts() {
    let names = ["api", "booking", "cache", "database"].map(str::to_owned);
    // Both cycles share api -> booking. The cheaper booking -> cache edge
    // breaks only the first cycle, so it must be restored after cutting the shared edge.
    let weights = [
        ((0, 1), 2),
        ((1, 2), 1),
        ((2, 0), 4),
        ((1, 3), 3),
        ((3, 0), 4),
    ];
    let edges = dependency_edges(&names, &weights);

    let original = DiGraph::<(), (), usize>::from_edges(edges.keys().copied());
    assert!(is_cyclic_directed(&original));
    let suggestions = compute_cycle_break_edges(&[0, 1, 2, 3], &names, &edges);
    assert_eq!(suggestions.len(), 1);
    let cut = &suggestions[0];
    assert_eq!(
        (cut.from.as_str(), cut.to.as_str(), cut.weight),
        ("api", "booking", 2)
    );
    let mut reported_pairs: Vec<_> = cut
        .file_edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    reported_pairs.sort_unstable();
    assert_eq!(
        reported_pairs,
        [
            ("api/source_1.rs", "booking/target_1.rs"),
            ("api/source_2.rs", "booking/target_2.rs"),
        ],
    );

    let remaining = DiGraph::<(), (), usize>::from_edges(
        edges
            .keys()
            .copied()
            .filter(|&(from, to)| names[from] != cut.from || names[to] != cut.to),
    );
    assert!(!is_cyclic_directed(&remaining));
}

#[test]
fn cycle_cut_prefers_the_cheapest_link_in_a_single_cycle() {
    let names = ["api", "booking", "cache"].map(str::to_owned);
    let edges = dependency_edges(&names, &[((0, 1), 4), ((1, 2), 1), ((2, 0), 3)]);

    let suggestions = compute_cycle_break_edges(&[0, 1, 2], &names, &edges);
    assert_eq!(suggestions.len(), 1);
    let cut = &suggestions[0];
    assert_eq!(
        (cut.from.as_str(), cut.to.as_str(), cut.weight),
        ("booking", "cache", 1)
    );
}

fn dependency_edges(
    names: &[String],
    weights: &[((usize, usize), usize)],
) -> BTreeMap<(usize, usize), Vec<DetailedFileEdge>> {
    weights
        .iter()
        .copied()
        .map(|((from, to), weight)| {
            let details = (1..=weight)
                .map(|index| DetailedFileEdge {
                    from: format!("{}/source_{index}.rs", names[from]),
                    to: format!("{}/target_{index}.rs", names[to]),
                    line: 1,
                    lines: vec![1],
                    symbols: Vec::new(),
                    kind: ImportKind::Import,
                    is_dip: false,
                })
                .collect();
            ((from, to), details)
        })
        .collect()
}
