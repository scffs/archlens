use std::{fs, path::Path};

use archlens::{AnalysisConfig, analyze, resolver::js_ts::load_tsconfig};

fn write(root: &Path, path: &str, contents: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, contents).unwrap();
}

#[test]
fn extends_and_references_prefer_the_exact_config_and_infer_missing_extensions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for (path, content) in [
        (
            "tsconfig.json",
            r#"{
                "extends": "./config/base",
                "references": [{ "path": "./config/app" }, { "path": "./config/extra" }]
            }"#,
        ),
        (
            "config/base",
            r#"{
                "extends": "./inherited",
                "compilerOptions": { "paths": { "alias/base": ["../src/base"] } }
            }"#,
        ),
        (
            "config/base.json",
            r#"{ "compilerOptions": { "paths": { "alias/base": ["../src/wrong"] } } }"#,
        ),
        (
            "config/inherited.json",
            r#"{ "compilerOptions": { "paths": { "alias/inherited": ["../src/inherited"] } } }"#,
        ),
        (
            "config/app",
            r#"{ "compilerOptions": { "paths": { "alias/app": ["../src/app"] } } }"#,
        ),
        (
            "config/app.json",
            r#"{ "compilerOptions": { "paths": { "alias/app": ["../src/wrong"] } } }"#,
        ),
        (
            "config/extra.json",
            r#"{ "compilerOptions": { "paths": { "alias/extra": ["../src/extra"] } } }"#,
        ),
        (
            "entry.ts",
            "import 'alias/base'; import 'alias/inherited'; import 'alias/app'; import 'alias/extra';",
        ),
    ] {
        write(root, path, content);
    }
    for name in ["app", "base", "extra", "inherited", "wrong"] {
        write(
            root,
            &format!("src/{name}.ts"),
            "export const ready = true;",
        );
    }

    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let report = analyze(&config).unwrap();
    let targets: Vec<_> = report
        .file_graph
        .edges
        .iter()
        .filter(|edge| edge.from == "entry.ts")
        .map(|edge| edge.to.as_str())
        .collect();

    assert_eq!(
        targets,
        [
            "src/app.ts",
            "src/base.ts",
            "src/extra.ts",
            "src/inherited.ts"
        ],
    );
}

#[test]
fn config_loads_the_last_allowed_parent_and_stops_before_the_next_reference() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for level in 0..6 {
        let next = level + 1;
        write(
            root,
            &format!("config-{level}.json"),
            &format!(r#"{{ "extends": "./config-{next}.json" }}"#),
        );
    }
    write(
        root,
        "config-6.json",
        r#"{
            "references": [{ "path": "./config-7.json" }],
            "compilerOptions": { "paths": { "available": ["./src/available"] } }
        }"#,
    );
    write(
        root,
        "config-7.json",
        r#"{ "compilerOptions": { "paths": { "too-deep": ["./src/too-deep"] } } }"#,
    );

    let config = load_tsconfig(root, &root.join("config-0.json"), "").unwrap();
    assert_eq!(
        config.paths,
        [("available".to_owned(), vec!["src/available".to_owned()])],
    );
}

#[test]
fn base_url_stands_on_its_own_and_references_stay_in_the_project() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let external = temp.path().join("external.json");
    write(
        temp.path(),
        "external.json",
        r#"{ "compilerOptions": { "paths": { "outside": ["./private"] } } }"#,
    );
    let config = serde_json::json!({
        "compilerOptions": { "baseUrl": "src" },
        "references": [{ "path": external }],
    });
    write(&root, "tsconfig.json", &config.to_string());
    write(&root, "empty.json", "{}");

    let config = load_tsconfig(&root, &root.join("tsconfig.json"), "").unwrap();
    assert!(config.has_base_url);
    assert_eq!(config.base_dir, "src");
    assert_eq!(config.paths, Vec::<(String, Vec<String>)>::new());
    assert!(load_tsconfig(&root, &root.join("empty.json"), "").is_none());
}
