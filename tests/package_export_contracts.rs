use std::fs;

use archlens::{AnalysisConfig, analyze};

#[test]
fn workspace_imports_prefer_root_exports_to_legacy_entries() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    for (folder, manifest, export) in [
        (
            "direct",
            r#"{ "name": "@workspace/direct", "exports": "./public.ts", "main": "./fallback.ts" }"#,
            "public.ts",
        ),
        (
            "conditional",
            r#"{
                "name": "@workspace/conditional",
                "exports": { ".": { "import": "./browser.ts", "require": "./fallback.ts" } },
                "main": "./fallback.ts"
            }"#,
            "browser.ts",
        ),
    ] {
        let path = root.join(folder);
        fs::create_dir(&path).unwrap();
        fs::write(path.join("package.json"), manifest).unwrap();
        fs::write(path.join(export), "export function selected() {}").unwrap();
        fs::write(path.join("fallback.ts"), "export function fallback() {}").unwrap();
    }
    fs::write(
        root.join("entry.ts"),
        "import '@workspace/direct'; import '@workspace/conditional';",
    )
    .unwrap();

    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let report = analyze(&config).expect("analyze workspace");
    let edges: Vec<_> = report
        .file_graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    assert_eq!(
        edges,
        [
            ("entry.ts", "conditional/browser.ts"),
            ("entry.ts", "direct/public.ts"),
        ],
    );
}
