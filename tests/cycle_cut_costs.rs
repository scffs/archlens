use std::collections::BTreeMap;

use archlens::ImportKind;
use archlens::domain::algorithms::compute_cycle_break_edges;
use archlens::domain::metrics::DetailedFileEdge;
use petgraph::algo::is_cyclic_directed;
use petgraph::graphmap::DiGraphMap;

#[test]
fn cycle_cuts_keep_the_total_cost_of_overlapping_cycles_low() {
    let names = ["api", "booking", "cache"].map(str::to_owned);
    let cases: &[(_, &[(_, _, _)], _)] = &[
        (
            "one shared dependency breaks both cycles",
            &[(0, 1, 3), (1, 0, 3), (1, 2, 3), (2, 0, 3)],
            3,
        ),
        (
            "two cuts are enough even with a cheaper alternative link",
            &[(0, 1, 2), (1, 0, 1), (1, 2, 6), (2, 0, 2), (2, 1, 5)],
            7,
        ),
    ];

    for &(case, weights, expected_cost) in cases {
        let mut edges = BTreeMap::new();
        for &(from, to, weight) in weights {
            let details = (0..weight)
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
            edges.insert((from, to), details);
        }

        let cuts = compute_cycle_break_edges(&[0, 1, 2], &names, &edges);
        let mut remaining = DiGraphMap::<&str, usize>::from_edges(
            weights
                .iter()
                .map(|&(from, to, weight)| (names[from].as_str(), names[to].as_str(), weight)),
        );
        assert!(is_cyclic_directed(&remaining), "{case}");
        let mut cost = 0;
        for cut in &cuts {
            cost += remaining
                .remove_edge(cut.from.as_str(), cut.to.as_str())
                .expect("each suggestion removes a distinct dependency");
        }
        assert!(!is_cyclic_directed(&remaining), "{case}");
        assert_eq!(cost, expected_cost, "{case}");
    }
}
