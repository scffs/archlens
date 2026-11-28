use std::{collections::BTreeMap, fs, path::Path};

use archlens::parser::parse_source;
use archlens::resolver::ProjectIndex;
use archlens::{AnalysisConfig, Language, analyze};

fn write_fixture(root: &Path, path: &str, source: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().expect("fixture parent")).expect("create fixture directory");
    fs::write(file, source).expect("write fixture");
}

#[test]
fn indexes_package_manifests_and_python_source_roots_outside_generated_directories() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    for (path, manifest) in [
        (
            "backend/pyproject.toml",
            "[project]\nname = 'billing'\nversion = '0.1.0'\n",
        ),
        (
            "native/Cargo.toml",
            "[package]\nname = 'native-job'\nversion = '0.1.0'\n",
        ),
        (
            "node_modules/dep/package.json",
            r#"{"name":"root-dependency"}"#,
        ),
        (
            "web/node_modules/dep/package.json",
            r#"{"name":"nested-dependency"}"#,
        ),
        (
            "target/package/cache/Cargo.toml",
            "[package]\nname = 'root-build-cache'\nversion = '0.1.0'\n",
        ),
        (
            "native/target/package/cache/Cargo.toml",
            "[package]\nname = 'nested-build-cache'\nversion = '0.1.0'\n",
        ),
    ] {
        write_fixture(root, path, manifest);
    }
    let sources = [
        (
            "backend/src/health.py",
            "def check():\n    return True\n",
            Language::Python,
        ),
        (
            "loose/src/handlers/entry.py",
            "from invoice import Invoice\n",
            Language::Python,
        ),
        (
            "loose/src/invoice.py",
            "class Invoice:\n    pass\n",
            Language::Python,
        ),
        ("native/src/lib.rs", "pub struct NativeJob;", Language::Rust),
    ];
    let parsed = sources.map(|(path, source, language)| {
        write_fixture(root, path, source);
        parse_source(path, source, language, false).expect("parse fixture")
    });
    let index = ProjectIndex::build(root, &parsed);
    let packages: BTreeMap<_, _> = index
        .packages
        .iter()
        .map(|manifest| (manifest.dir.as_str(), manifest.name.as_str()))
        .collect();
    assert_eq!(
        packages,
        BTreeMap::from([("backend", "billing"), ("native", "native-job")])
    );
    assert_eq!(
        index.python_roots,
        ["", "backend", "backend/src", "loose/src"]
    );
    let native = index
        .enclosing_rust_crate("native/src/lib.rs")
        .expect("native crate");
    assert_eq!(native.crate_ident, "native_job");
    assert_eq!(native.lib_entry.as_deref(), Some("native/src/lib.rs"));

    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let graph = analyze(&config).expect("analyze Python import").file_graph;
    assert!(graph.edges.iter().any(|edge| {
        edge.from == "loose/src/handlers/entry.py"
            && edge.to == "loose/src/invoice.py"
            && edge.symbols == ["Invoice"]
    }));
}

#[test]
fn indexes_symbol_kinds_and_distinguishes_named_and_wildcard_reexports() {
    let temp = tempfile::tempdir().expect("tempdir");
    let parsed = [
        (
            "src/provider.ts",
            "export interface Provider {}\nexport class FileProvider {}",
        ),
        ("src/named.ts", "export { Provider } from './provider';"),
        ("src/barrel.ts", "export * from './provider';"),
    ]
    .map(|(path, source)| {
        parse_source(path, source, Language::TypeScript, false).expect("parse fixture")
    });
    let index = ProjectIndex::build(temp.path(), &parsed);

    assert!(index.file_declares_symbol("src/provider.ts", "FileProvider"));
    assert!(!index.file_declares_symbol("src/provider.ts", "Missing"));
    assert!(index.file_declares_abstract("src/provider.ts", "Provider"));
    assert!(!index.file_declares_abstract("src/provider.ts", "FileProvider"));
    assert_eq!(
        index.reexports,
        [(
            ("src/named.ts".into(), "Provider".into()),
            "src/provider.ts".into()
        )]
        .into()
    );
    assert_eq!(
        index.wildcard_reexports,
        [("src/barrel.ts".into(), vec!["src/provider.ts".into()])].into()
    );
}
