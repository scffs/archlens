use std::collections::HashMap;

use archlens::domain::analyzer::analyze_view;
use archlens::domain::projection::ViewProjection;
use archlens::{FileGraph, FileNode, Language, SymbolCounts};

#[test]
fn group_sums_file_lines_and_uses_the_cluster_of_most_files() {
    let files = [
        ("group/large.ts", 9, 0),
        ("group/entry.ts", 1, 1),
        ("group/helper.ts", 3, 1),
    ];
    let graph = FileGraph {
        files: files
            .iter()
            .map(|&(path, loc, _)| FileNode {
                path: path.into(),
                language: Language::TypeScript,
                package: None,
                package_root: None,
                module_id: "group".into(),
                loc,
                symbols: SymbolCounts::default(),
                declarations: Vec::new(),
                external_imports: Vec::new(),
                unresolved_imports: Vec::new(),
                git: None,
            })
            .collect(),
        edges: Vec::new(),
    };
    let projection = ViewProjection {
        id: "projected".into(),
        title: "Projected files".into(),
        description: String::new(),
        file_to_group: files
            .iter()
            .map(|&(path, _, _)| (path.into(), "group".into()))
            .collect(),
    };
    let clusters: HashMap<_, _> = files
        .into_iter()
        .map(|(path, _, cluster)| (path.into(), cluster))
        .collect();

    let view = analyze_view(&graph, &projection, 0.15, 0.05, &clusters, &[]);
    let [group] = view.nodes.as_slice() else {
        panic!("expected one projected group");
    };

    assert_eq!(group.loc, 13);
    assert_eq!(group.louvain_cluster, 1);
}
