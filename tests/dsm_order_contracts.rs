use std::path::Path;

use archlens::{
    AnalysisConfig, FileEdge, FileGraph, FileNode, ImportKind, Language, SymbolCounts,
    analyze_file_graph,
};

fn dsm_order(files: &[&str], dependencies: &[(&str, &str)]) -> Vec<String> {
    let graph = FileGraph {
        files: files
            .iter()
            .map(|&path| FileNode {
                path: path.into(),
                language: Language::Rust,
                package: None,
                package_root: None,
                module_id: String::new(),
                loc: 1,
                symbols: SymbolCounts::default(),
                declarations: Vec::new(),
                external_imports: Vec::new(),
                unresolved_imports: Vec::new(),
                git: None,
            })
            .collect(),
        edges: dependencies
            .iter()
            .map(|&(from, to)| FileEdge {
                from: from.into(),
                to: to.into(),
                kind: ImportKind::Import,
                line: 1,
                lines: vec![1],
                symbols: Vec::new(),
                is_dip: false,
            })
            .collect(),
    };
    let root = Path::new(".");
    let report =
        analyze_file_graph(root, &graph, &AnalysisConfig::new(root)).expect("analyze graph");
    let view = report.view("files").expect("file view");
    view.dsm_order.clone()
}

#[test]
fn dsm_groups_cycles_and_orders_ready_components_by_stability() {
    let files = [
        "a-core.rs",
        "a-peer.rs",
        "b-left.rs",
        "b-right.rs",
        "isolated.rs",
        "one.rs",
        "two.rs",
        "three.rs",
        "four.rs",
        "five.rs",
    ];
    let dependencies = [
        ("a-core.rs", "a-peer.rs"),
        ("a-peer.rs", "a-core.rs"),
        ("b-left.rs", "b-right.rs"),
        ("b-right.rs", "b-left.rs"),
        ("one.rs", "a-core.rs"),
        ("one.rs", "b-left.rs"),
        ("two.rs", "a-core.rs"),
        ("two.rs", "b-left.rs"),
        ("three.rs", "a-core.rs"),
        ("three.rs", "b-right.rs"),
        ("four.rs", "a-core.rs"),
        ("four.rs", "b-right.rs"),
        ("five.rs", "a-core.rs"),
    ];

    let order = dsm_order(&files, &dependencies);
    assert_eq!(order.len(), files.len());
    assert_eq!(order[..2], ["b-left.rs", "b-right.rs"]);
    assert_eq!(order[2..4], ["a-core.rs", "a-peer.rs"]);
    assert_eq!(order.last().map(String::as_str), Some("isolated.rs"));
    let position = |path: &str| {
        order
            .iter()
            .position(|file| file == path)
            .expect("file ranked")
    };
    for (consumer, provider) in dependencies.into_iter().skip(4) {
        assert!(
            position(provider) < position(consumer),
            "{consumer} uses {provider}"
        );
    }
}

#[test]
fn dsm_prioritizes_shared_providers_when_their_stability_matches() {
    let files = ["a-local.rs", "z-shared.rs", "one.rs", "two.rs", "three.rs"];
    let dependencies = [
        ("one.rs", "a-local.rs"),
        ("two.rs", "z-shared.rs"),
        ("three.rs", "z-shared.rs"),
    ];

    let order = dsm_order(&files, &dependencies);
    assert_eq!(order.len(), files.len());
    assert_eq!(order[..2], ["z-shared.rs", "a-local.rs"]);
}

#[test]
fn dsm_compares_average_instability_across_different_cycle_sizes() {
    let files = [
        "a-left.rs",
        "a-right.rs",
        "b-first.rs",
        "b-second.rs",
        "b-third.rs",
        "one.rs",
        "two.rs",
    ];
    let dependencies = [
        ("a-left.rs", "a-right.rs"),
        ("a-right.rs", "a-left.rs"),
        ("b-first.rs", "b-second.rs"),
        ("b-second.rs", "b-third.rs"),
        ("b-third.rs", "b-first.rs"),
        ("one.rs", "a-left.rs"),
        ("one.rs", "a-right.rs"),
        ("one.rs", "b-first.rs"),
        ("one.rs", "b-second.rs"),
        ("one.rs", "b-third.rs"),
        ("two.rs", "b-first.rs"),
        ("two.rs", "b-second.rs"),
        ("two.rs", "b-third.rs"),
    ];

    let order = dsm_order(&files, &dependencies);
    // Each B member has instability 1/4, compared with 1/3 for each A member.
    // B is more stable on average even though its total instability is higher.
    assert_eq!(order[..3], ["b-first.rs", "b-second.rs", "b-third.rs"]);
}
