use archlens::domain::projection::{assign_auto_module_ids, build_projections};
use archlens::{AnalysisConfig, FileGraph, FileNode, Language, SymbolCounts};

fn file(path: &str, package: Option<(&str, &str)>) -> FileNode {
    FileNode {
        path: path.into(),
        language: Language::Rust,
        package: package.map(|(name, _)| name.into()),
        package_root: package.map(|(_, root)| root.into()),
        module_id: String::new(),
        loc: 1,
        symbols: SymbolCounts::default(),
        declarations: Vec::new(),
        external_imports: Vec::new(),
        unresolved_imports: Vec::new(),
        git: None,
    }
}

#[test]
fn projects_manifest_packages_and_folders_at_positive_depths() {
    let app = "packages/app/src/feature/client.rs";
    let core = "packages/core/lib.rs";
    let standalone = "standalone.rs";
    let mut graph = FileGraph {
        files: vec![
            file(app, Some(("app", "packages/app"))),
            file(core, Some(("core", "packages/core"))),
            file(standalone, None),
        ],
        edges: Vec::new(),
    };
    let mut config = AnalysisConfig::new(".");
    config.folder_depths = vec![0, 6, 1, 1];

    let views = build_projections(&graph, &config).expect("build views");
    let names: Vec<_> = views.iter().map(|view| view.id.as_str()).collect();
    assert_eq!(
        names,
        ["modules", "packages", "folders-d1", "folders-d6", "files"]
    );
    let group = |view: &str, path: &str| {
        views
            .iter()
            .find(|entry| entry.id == view)
            .expect("find view")
            .file_to_group
            .get(path)
            .expect("group file")
            .as_str()
    };
    assert_eq!(group("modules", app), "app/feature");
    assert_eq!(group("modules", core), "core");
    assert_eq!(group("packages", standalone), "<root>");
    assert_eq!(group("packages", app), "app");
    assert_eq!(group("folders-d1", app), "packages");
    assert_eq!(group("folders-d6", app), "packages/app/src/feature");
    assert_eq!(group("folders-d6", standalone), "<root>");

    graph.files.truncate(2);
    let views_for_two = build_projections(&graph, &config).expect("two packages");
    assert!(views_for_two.iter().any(|view| view.id == "packages"));

    graph.files.truncate(1);
    let views_for_one = build_projections(&graph, &config).expect("one package");
    assert!(!views_for_one.iter().any(|view| view.id == "packages"));
}

#[test]
fn groups_busy_package_roots_while_preserving_entrypoints_and_paired_modules() {
    let package = Some(("core", "packages/core"));
    let mut graph = FileGraph {
        files: vec![
            file("packages/core/src/main.rs", package),
            file("packages/core/src/feature.rs", package),
            file("packages/core/src/feature/worker.rs", package),
        ],
        edges: Vec::new(),
    };
    graph.files.extend(
        (1..=11).map(|index| file(&format!("packages/core/src/helper_{index}.rs"), package)),
    );

    assign_auto_module_ids(&mut graph);
    assert_eq!(graph.files[0].module_id, "main");
    assert_eq!(graph.files[1].module_id, "feature");
    assert_eq!(graph.files[2].module_id, "feature");
    assert!(
        graph.files[3..]
            .iter()
            .all(|node| node.module_id == "(root)")
    );
}
