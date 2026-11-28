use archlens::domain::algorithms::detect_louvain_and_misplaced;
use archlens::{FileEdge, FileGraph, FileNode, ImportKind, Language, SymbolCounts};

fn graph_from_imports(paths: &[&str], imports: &[(usize, usize, usize)]) -> FileGraph {
    FileGraph {
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
        edges: imports
            .iter()
            .map(|&(from, to, count)| FileEdge {
                from: paths[from].to_owned(),
                to: paths[to].to_owned(),
                kind: ImportKind::Import,
                line: 1,
                lines: vec![1],
                symbols: (0..count).map(|index| format!("symbol_{index}")).collect(),
                is_dip: false,
            })
            .collect(),
    }
}

#[test]
fn louvain_groups_a_helper_with_its_stronger_coupling_in_either_direction() {
    let paths = [
        "identity/user.rs",
        "identity/policy.rs",
        "billing/invoice.rs",
        "billing/payment.rs",
        "shared/helper.rs",
    ];

    for (left_symbols, right_symbols, expected_neighbor) in [(0, 6, 2), (6, 0, 0)] {
        let imports = [
            (0, 1, 4),
            (2, 3, 4),
            (0, 4, left_symbols),
            (2, 4, right_symbols),
        ];
        for directed_imports in [imports, imports.map(|(from, to, count)| (to, from, count))] {
            let graph = graph_from_imports(&paths, &directed_imports);
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

#[test]
fn louvain_keeps_two_groups_with_a_small_import_between_them() {
    let paths = [
        "identity/user.rs",
        "identity/policy.rs",
        "billing/invoice.rs",
        "billing/payment.rs",
    ];
    let imports = [(0, 1, 0), (2, 3, 0), (0, 2, 2)];

    for directed_imports in [imports, imports.map(|(from, to, count)| (to, from, count))] {
        let graph = graph_from_imports(&paths, &directed_imports);
        let (clusters, _) = detect_louvain_and_misplaced(&graph);

        assert_eq!(clusters[paths[0]], clusters[paths[1]]);
        assert_eq!(clusters[paths[2]], clusters[paths[3]]);
        assert_ne!(clusters[paths[0]], clusters[paths[2]]);
    }
}

#[test]
fn louvain_groups_direct_imports_of_different_weights_around_one_file() {
    let paths = [
        "checkout/order.rs",
        "checkout/audit.rs",
        "checkout/receipt.rs",
    ];
    let graph = graph_from_imports(&paths, &[(0, 1, 0), (0, 2, 6)]);
    let (clusters, _) = detect_louvain_and_misplaced(&graph);

    assert_eq!(clusters[paths[0]], clusters[paths[2]]);
    assert_eq!(clusters[paths[0]], clusters[paths[1]]);
}
