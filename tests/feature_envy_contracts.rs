use std::path::Path;

use archlens::{
    AnalysisConfig, FileEdge, FileGraph, FileNode, ImportKind, Language, MisplacedFile,
    SymbolCounts, analyze_file_graph,
};

const MOVER: &str = "source/mover.rs";
const FILES: &[&str] = &[
    MOVER,
    "source/peer.rs",
    "source/idle.rs",
    "target/a.rs",
    "target/b.rs",
    "target/c.rs",
    "spare/a.rs",
    "spare/b.rs",
];
const OUTGOING: &[(&str, &str)] = &[
    (MOVER, "target/a.rs"),
    (MOVER, "target/b.rs"),
    (MOVER, "target/c.rs"),
    (MOVER, "source/peer.rs"),
    (MOVER, "spare/a.rs"),
];

fn recommendations(files: &[&str], edges: &[(&str, &str)]) -> Vec<MisplacedFile> {
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
        edges: edges
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
    analyze_file_graph(root, &graph, &AnalysisConfig::new(root))
        .expect("analyze graph")
        .misplaced_files
}

#[test]
fn relocation_requires_distinct_targets_and_accepts_the_affinity_boundary() {
    let at_boundary = recommendations(FILES, OUTGOING);
    let [recommendation] = at_boundary.as_slice() else {
        panic!("expected exactly one recommendation, got {at_boundary:?}");
    };
    assert_eq!(recommendation.file, MOVER);
    assert_eq!(recommendation.current_module, "source");
    assert_eq!(recommendation.suggested_module, "target");
    assert_eq!(recommendation.internal_edges, 1);
    assert_eq!(recommendation.suggested_module_edges, 3);
    assert!((recommendation.affinity - 0.60).abs() < 1e-9);

    let only_targets = &OUTGOING[..3];
    assert_eq!(recommendations(FILES, only_targets).len(), 1);
    let repeated_targets = [OUTGOING[0], OUTGOING[0], OUTGOING[1]];
    assert!(recommendations(FILES, &repeated_targets).is_empty());

    let below_affinity = [OUTGOING, &[(MOVER, "spare/b.rs")]].concat();
    assert!(recommendations(FILES, &below_affinity).is_empty());

    let stronger_current_module = [&OUTGOING[..4], &[(MOVER, "source/idle.rs")]].concat();
    assert!(recommendations(FILES, &stronger_current_module).is_empty());
}

#[test]
fn relocation_spares_small_modules_and_cores_shared_between_modules() {
    let small_module: Vec<_> = FILES
        .iter()
        .copied()
        .filter(|&file| file != "source/idle.rs")
        .collect();
    assert!(recommendations(&small_module, OUTGOING).is_empty());

    let same_consumer = [
        OUTGOING,
        &[
            ("target/a.rs", MOVER),
            ("target/b.rs", MOVER),
            ("source/peer.rs", MOVER),
        ],
    ]
    .concat();
    assert_eq!(recommendations(FILES, &same_consumer).len(), 1);

    let independent_consumers = [same_consumer.as_slice(), &[("spare/a.rs", MOVER)]].concat();
    assert!(recommendations(FILES, &independent_consumers).is_empty());
}

#[test]
fn recommendations_rank_by_target_count_affinity_and_file_path() {
    let tied = [
        "target/a.rs",
        "target/b.rs",
        "target/c.rs",
        "source/peer.rs",
    ];
    let candidates: [(&str, &[&str]); 4] = [
        ("source/beta.rs", &tied),
        ("source/alpha.rs", &tied),
        ("source/focused.rs", &tied[..3]),
        (
            "source/broad.rs",
            &[
                "target/a.rs",
                "target/b.rs",
                "target/c.rs",
                "target/d.rs",
                "source/peer.rs",
                "spare/a.rs",
            ],
        ),
    ];
    let files: Vec<_> = FILES
        .iter()
        .copied()
        .chain(candidates.iter().map(|&(file, _)| file))
        .chain(["target/d.rs"])
        .collect();
    let edges: Vec<_> = candidates
        .iter()
        .flat_map(|&(file, targets)| targets.iter().map(move |&target| (file, target)))
        .collect();

    let result = recommendations(&files, &edges);
    let order: Vec<_> = result.iter().map(|entry| entry.file.as_str()).collect();
    assert_eq!(
        order,
        [
            "source/broad.rs",
            "source/focused.rs",
            "source/alpha.rs",
            "source/beta.rs",
        ]
    );
}

#[test]
fn self_and_dangling_edges_do_not_change_relocation_recommendations() {
    let edges = [
        OUTGOING,
        &[
            (MOVER, MOVER),
            (MOVER, "missing/target.rs"),
            ("missing/consumer.rs", MOVER),
        ],
    ]
    .concat();

    let result = recommendations(FILES, &edges);
    let [recommendation] = result.as_slice() else {
        panic!("expected one recommendation at the affinity boundary, got {result:?}");
    };
    assert_eq!(recommendation.file, MOVER);
    assert_eq!(recommendation.internal_edges, 1);
    assert!((recommendation.affinity - 0.60).abs() < 1e-9);
}
