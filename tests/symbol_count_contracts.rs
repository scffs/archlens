use std::fs;

use archlens::{AnalysisConfig, SymbolCounts, analyze};

#[test]
fn module_abstractness_uses_exports_and_counts_private_only_files() {
    let project = tempfile::tempdir().expect("create source fixture");
    let api = project.path().join("src/api");
    fs::create_dir_all(&api).expect("create module");
    fs::write(
        api.join("public.rs"),
        concat!(
            "pub trait Port {}\n",
            "pub struct Adapter;\npub struct Runner;\npub struct Driver;\npub struct Client;\n",
            "fn helper() {}\nfn other_helper() {}\nfn third_helper() {}\n",
        ),
    )
    .expect("write public declarations");
    fs::write(
        api.join("bootstrap.rs"),
        "trait LocalPort {}\nfn boot() {}\nfn retry() {}\n",
    )
    .expect("write private-only declarations");

    let mut config = AnalysisConfig::new(project.path());
    config.git.enabled = false;
    let report = analyze(&config).expect("analyze Rust module");
    let modules = report.view("modules").expect("modules view");
    let api = modules
        .nodes
        .iter()
        .find(|node| node.id == "api")
        .expect("api module");
    assert_eq!(
        api.symbols,
        SymbolCounts {
            abstract_declarations: 2,
            concrete_declarations: 6,
            private_declarations: 3,
        },
    );
    assert_eq!(api.abstractness, Some(0.25));
}
