use std::fs;

use archlens::{AnalysisConfig, analyze};
use tempfile::TempDir;

#[test]
fn cycle_reports_count_and_sample_only_edges_inside_the_cycle() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    for folder in ["entry", "left", "right", "tail"] {
        fs::create_dir(root.join(folder)).expect("create fixture folder");
    }

    // Thirteen file references collapse into one folder edge, exceeding the cycle's sample limit.
    for index in 0..13 {
        fs::write(
            root.join(format!("left/item{index}.ts")),
            "import '../right/index';\n",
        )
        .expect("write left fixture");
    }
    for (file, source) in [
        ("entry/index.ts", "import '../left/item0';\n"),
        (
            "right/index.ts",
            "import '../left/item0';\nimport '../tail/index';\n",
        ),
        ("tail/index.ts", "export const tail = 1;\n"),
    ] {
        fs::write(root.join(file), source).expect("write fixture");
    }

    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let report = analyze(&config).expect("analyze cycle fixture");
    let view = report
        .views
        .iter()
        .find(|view| view.id == "folders-d1")
        .expect("first-level folder view");

    let mut edges: Vec<_> = view
        .edges
        .iter()
        .map(|edge| {
            (
                edge.from.as_str(),
                edge.to.as_str(),
                edge.weight,
                edge.is_cycle_edge,
            )
        })
        .collect();
    edges.sort_unstable();
    assert_eq!(
        edges,
        [
            ("entry", "left", 1, false),
            ("left", "right", 13, true),
            ("right", "left", 1, true),
            ("right", "tail", 1, false),
        ]
    );

    assert_eq!(view.cycles.len(), 1);
    let cycle = &view.cycles[0];
    assert_eq!(cycle.nodes, ["left", "right"]);
    assert_eq!(cycle.edge_count, 2);
    assert_eq!(cycle.sample_file_edges.len(), 12);
    assert!(cycle.sample_file_edges.iter().all(|(from, to)| {
        (from.starts_with("left/") && to == "right/index.ts")
            || (from == "right/index.ts" && to == "left/item0.ts")
    }));
}
