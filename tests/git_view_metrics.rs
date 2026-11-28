use std::path::Path;

use archlens::{
    AnalysisConfig, ArchitectureZone, FileEdge, FileGitMetrics, FileGraph, FileNode, ImportKind,
    Language, SymbolCounts, analyze_file_graph,
};

fn file_with_history(
    path: &str,
    commits: usize,
    churn: (usize, usize),
    hotspot: f64,
    owner: (&str, f64),
) -> FileNode {
    FileNode {
        path: path.into(),
        language: Language::Rust,
        package: None,
        package_root: None,
        module_id: String::new(),
        loc: 1,
        symbols: SymbolCounts {
            concrete_declarations: 1,
            ..SymbolCounts::default()
        },
        declarations: Vec::new(),
        external_imports: Vec::new(),
        unresolved_imports: Vec::new(),
        git: Some(FileGitMetrics {
            commits,
            lines_added: churn.0,
            lines_deleted: churn.1,
            top_author: owner.0.into(),
            top_author_share: owner.1,
            hotspot_score: hotspot,
            ..FileGitMetrics::default()
        }),
    }
}

#[test]
fn folder_report_aggregates_git_activity_and_marks_active_pain_at_the_cutoff() {
    let graph = FileGraph {
        files: vec![
            file_with_history("core/port.rs", 8, (12, 3), 0.30, ("Alice", 0.5)),
            file_with_history("core/model.rs", 4, (5, 9), 0.12, ("Alice", 0.5)),
            file_with_history("core/store.rs", 5, (2, 7), 0.20, ("Bob", 0.8)),
            file_with_history("core/legacy.rs", 3, (1, 1), 0.25, ("", 0.0)),
            file_with_history("cold/cache.rs", 2, (2, 1), 0.29, ("Bob", 1.0)),
            file_with_history("entry/main.rs", 9, (7, 2), 0.95, ("Alice", 1.0)),
        ],
        edges: ["core/port.rs", "cold/cache.rs"]
            .map(|target| FileEdge {
                from: "entry/main.rs".into(),
                to: target.into(),
                kind: ImportKind::Import,
                line: 1,
                lines: vec![1],
                symbols: Vec::new(),
                is_dip: false,
            })
            .into(),
    };
    let root = Path::new(".");
    let report = analyze_file_graph(root, &graph, &AnalysisConfig::new(root))
        .expect("analyze file graph with supplied Git history");
    let view = report.view("folders-d1").expect("first-level folder view");
    let folder = |id: &str| {
        view.nodes
            .iter()
            .find(|node| node.id == id)
            .expect("folder")
    };

    let core = folder("core");
    assert_eq!(core.commits, 20);
    assert_eq!(core.churn, 40);
    assert!((core.hotspot_score - 0.30).abs() < f64::EPSILON);
    assert_eq!(core.top_author, "Alice");
    // Alice accounts for six of the ten attributed touches. Anonymous churn is still reported.
    assert!((core.top_author_share - 0.60).abs() < f64::EPSILON);
    assert_eq!(core.zone, ArchitectureZone::ZoneOfPain);
    assert!(core.active_pain);

    let cold = folder("cold");
    assert_eq!(cold.zone, ArchitectureZone::ZoneOfPain);
    assert!(!cold.active_pain);

    let entry = folder("entry");
    assert_eq!(entry.zone, ArchitectureZone::MainSequence);
    assert!(!entry.active_pain);
}
