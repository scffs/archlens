use std::path::Path;

use archlens::{
    AnalysisConfig, ArchitectureView, ArchitectureZone, FileEdge, FileGitMetrics, FileGraph,
    FileNode, ImportKind, Language, SymbolCounts, analyze_file_graph,
};

#[test]
fn summary_counts_architecture_zones_and_averages_defined_metrics() {
    let graph = FileGraph {
        files: [
            ("calm.ts", 1, 0),
            ("pain.ts", 0, 1),
            ("useless.ts", 3, 1),
            ("isolated.ts", 0, 1),
            ("empty.ts", 0, 0),
            ("hub.ts", 1, 3),
            ("peer.ts", 1, 1),
        ]
        .into_iter()
        .map(
            |(name, abstract_declarations, concrete_declarations)| FileNode {
                path: name.into(),
                language: Language::TypeScript,
                package: None,
                package_root: None,
                module_id: String::new(),
                loc: 1,
                symbols: SymbolCounts {
                    abstract_declarations,
                    concrete_declarations,
                    private_declarations: 0,
                },
                declarations: Vec::new(),
                external_imports: Vec::new(),
                unresolved_imports: Vec::new(),
                git: (name == "pain.ts").then_some(FileGitMetrics {
                    hotspot_score: 0.5,
                    ..FileGitMetrics::default()
                }),
            },
        )
        .collect(),
        edges: [
            ("hub.ts", "calm.ts"),
            ("hub.ts", "pain.ts"),
            ("hub.ts", "peer.ts"),
            ("peer.ts", "hub.ts"),
            ("useless.ts", "hub.ts"),
            ("empty.ts", "calm.ts"),
        ]
        .into_iter()
        .map(|(from, to)| FileEdge {
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

    let view = file_view(&graph);
    for (file, expected_zone) in [
        ("calm.ts", ArchitectureZone::MainSequence),
        ("pain.ts", ArchitectureZone::ZoneOfPain),
        ("useless.ts", ArchitectureZone::ZoneOfUselessness),
        ("isolated.ts", ArchitectureZone::Isolated),
        ("empty.ts", ArchitectureZone::NoEntities),
    ] {
        let node = view
            .nodes
            .iter()
            .find(|node| node.id == file)
            .expect("node");
        assert_eq!(node.zone, expected_zone, "{file}");
    }

    let summary = view.summary;
    assert_eq!(summary.node_count, 7);
    assert_eq!(summary.nodes_in_cycles, 2);
    assert_eq!(summary.main_sequence_count, 3);
    assert_eq!(summary.zone_of_pain_count, 1);
    assert_eq!(summary.active_pain_count, 1);
    assert_eq!(summary.zone_of_uselessness_count, 1);
    assert_eq!(summary.isolated_count, 1);
    assert_eq!(summary.no_entities_count, 1);

    // The isolated file has symbols but no coupling, while the empty file has coupling but no symbols.
    assert_eq!(summary.average_distance, Some(0.38));
    assert_eq!(summary.max_distance, Some(1.0));
    assert_eq!(summary.average_instability, Some(0.5167));
    assert_eq!(summary.average_abstractness, Some(0.4167));
}

#[test]
fn summary_omits_metrics_without_observations() {
    let summary = file_view(&FileGraph::default()).summary;
    assert_eq!(summary.node_count, 0);
    assert_eq!(summary.average_distance, None);
    assert_eq!(summary.max_distance, None);
    assert_eq!(summary.average_instability, None);
    assert_eq!(summary.average_abstractness, None);
}

fn file_view(graph: &FileGraph) -> ArchitectureView {
    let root = Path::new(".");
    let report =
        analyze_file_graph(root, graph, &AnalysisConfig::new(root)).expect("analyze file graph");
    report
        .views
        .into_iter()
        .find(|view| view.id == "files")
        .expect("file view")
}
