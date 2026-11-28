use std::{fs, path::Path};

use archlens::{AnalysisConfig, ImportKind, analyze};

fn write_fixture(root: &Path, files: &[(&str, &str)]) {
    for (path, source) in files {
        let file = root.join(path);
        fs::create_dir_all(file.parent().expect("parent directory"))
            .expect("create fixture directory");
        fs::write(file, source).expect("write fixture source");
    }
}

#[test]
fn resolves_rust_paths_from_the_custom_library_entry_and_through_reexports() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_fixture(
        root,
        &[
            (
                "Cargo.toml",
                "[package]\nname = 'library-fixture'\nversion = '0.1.0'\nedition = '2024'\n[lib]\npath = 'library/lib.rs'\n",
            ),
            (
                "library/lib.rs",
                "pub mod consumer;\npub mod facade;\npub mod feature;\nmod model;\npub struct RootType;\npub use model::*;",
            ),
            (
                "library/model.rs",
                "pub struct Record;\npub struct RootRecord;",
            ),
            ("library/facade.rs", "pub use crate::model::Record;"),
            (
                "library/consumer.rs",
                "use crate::{RootType, RootRecord, facade::Record};",
            ),
            ("library/feature.rs", "pub mod deeper;\npub struct Feature;"),
            ("library/feature/deeper.rs", "pub mod entry;"),
            (
                "library/feature/deeper/entry.rs",
                "use super::super as ancestor;\nuse super::super::Feature;\nuse crate::*;",
            ),
        ],
    );
    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    config.include_mod_decls = true;
    let graph = analyze(&config).expect("analyze Rust crate").file_graph;
    let edge = |from: &str, to: &str| {
        graph
            .edges
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .unwrap_or_else(|| panic!("missing {from} -> {to}, graph: {:?}", graph.edges))
    };

    assert_eq!(
        edge("library/lib.rs", "library/feature.rs").kind,
        ImportKind::ModuleDecl
    );
    assert_eq!(
        edge("library/consumer.rs", "library/lib.rs").symbols,
        ["RootType"]
    );
    assert_eq!(
        edge("library/consumer.rs", "library/model.rs").symbols,
        ["Record", "RootRecord"]
    );
    let nested = "library/feature/deeper/entry.rs";
    assert_eq!(edge(nested, "library/feature.rs").lines, [1, 2]);
    assert_eq!(edge(nested, "library/lib.rs").lines, [3]);
}

#[test]
fn keeps_a_production_module_with_the_same_name_as_a_nested_test_module() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write_fixture(
        root,
        &[
            (
                "standalone/src/lib.rs",
                "pub mod billing;\npub mod definitions;\npub mod diagnostics;",
            ),
            (
                "standalone/src/billing.rs",
                "#[cfg(test)]\nmod diagnostics;\nuse crate::definitions::Invoice;\npub fn archive(_: Invoice) {}",
            ),
            ("standalone/src/definitions.rs", "pub struct Invoice;"),
            ("standalone/src/diagnostics.rs", "pub struct Log;"),
            ("standalone/src/billing/diagnostics/mod.rs", "mod helpers;"),
            (
                "standalone/src/billing/diagnostics/helpers.rs",
                "pub struct TestLog;",
            ),
        ],
    );
    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let graph = analyze(&config)
        .expect("analyze standalone source")
        .file_graph;
    let files: Vec<_> = graph.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        files,
        [
            "standalone/src/billing.rs",
            "standalone/src/definitions.rs",
            "standalone/src/diagnostics.rs",
            "standalone/src/lib.rs",
        ]
    );
    assert!(graph.edges.iter().any(|edge| {
        edge.from == "standalone/src/billing.rs"
            && edge.to == "standalone/src/definitions.rs"
            && edge.symbols == ["Invoice"]
    }));
}
