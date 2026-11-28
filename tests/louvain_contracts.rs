use archlens::domain::algorithms::detect_louvain_and_misplaced;
use archlens::{FileEdge, FileGraph, FileNode, ImportKind, Language, SymbolCounts};

#[test]
fn louvain_groups_a_helper_with_its_stronger_coupling_in_either_direction() {
    let paths = [
        "identity/user.rs",
        "identity/policy.rs",
        "billing/invoice.rs",
        "billing/payment.rs",
        "shared/helper.rs",
    ];
    let mut graph = FileGraph {
        files: paths
            .iter()
            .map(|&path| FileNode {
                path: path.to_owned(),
                language: Language::Rust,
                package: None,
                package_root: None,
                module_id: path.split_once('/').expect("module path").0.to_owned(),
                loc: 1,
                symbols: SymbolCounts::default(),
                declarations: Vec::new(),
                external_imports: Vec::new(),
                unresolved_imports: Vec::new(),
                git: None,
            })
            .collect(),
        edges: Vec::new(),
    };

    for (left_symbols, right_symbols, expected_neighbor) in [(0, 6, 2), (6, 0, 0)] {
        for reverse in [false, true] {
            graph.edges = [
                (0, 1, 4),
                (2, 3, 4),
                (0, 4, left_symbols),
                (2, 4, right_symbols),
            ]
            .into_iter()
            .map(|(from, to, count)| {
                let (from, to) = if reverse { (to, from) } else { (from, to) };
                FileEdge {
                    from: paths[from].to_owned(),
                    to: paths[to].to_owned(),
                    kind: ImportKind::Import,
                    line: 1,
                    lines: vec![1],
                    symbols: (0..count).map(|index| format!("symbol_{index}")).collect(),
                    is_dip: false,
                }
            })
            .collect();

            let (clusters, _) = detect_louvain_and_misplaced(&graph);
            assert_eq!(clusters[paths[0]], clusters[paths[1]]);
            assert_eq!(clusters[paths[2]], clusters[paths[3]]);
            assert_ne!(clusters[paths[0]], clusters[paths[2]]);
            assert_eq!(
                clusters[paths[4]], clusters[paths[expected_neighbor]],
                "helper with {left_symbols} identity and {right_symbols} billing imports"
            );
        }
    }
}
