use archlens::resolver::js_ts::resolve_js_ts_import;
use archlens::resolver::{ProjectIndex, ResolveOutcome};

#[test]
fn javascript_family_resolution_preserves_file_and_extension_precedence() {
    let mut index = ProjectIndex::default();
    index.known_files.extend(
        [
            "src/exact.js",
            "src/exact.ts",
            "src/mapped.ts",
            "src/mapped.tsx",
            "src/file.ts",
            "src/file.js",
            "src/file/index.ts",
            "src/directory/index.tsx",
            "src/directory/index.js",
            "src/vue/index.vue",
        ]
        .map(str::to_owned),
    );

    for (specifier, target) in [
        ("./exact.js", "src/exact.js"),
        ("./mapped.js", "src/mapped.ts"),
        ("./file", "src/file.ts"),
        ("./directory", "src/directory/index.tsx"),
        ("./vue", "src/vue/index.vue"),
    ] {
        assert_eq!(
            resolve_js_ts_import(&index, "src/entry.ts", specifier),
            ResolveOutcome::Internal(target.to_owned()),
            "{specifier}",
        );
    }
}

#[test]
fn referenced_config_paths_keep_their_own_base_directory() {
    use archlens::{AnalysisConfig, analyze};
    use std::fs;

    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    let site = root.join("site");
    fs::create_dir_all(site.join(".generated")).expect("generated config directory");
    fs::create_dir_all(site.join("features")).expect("feature directory");
    fs::write(
        site.join("tsconfig.json"),
        r#"{ "references": [{ "path": "./.generated/tsconfig.app.json" }] }"#,
    )
    .expect("write project references");
    fs::write(
        site.join(".generated/tsconfig.app.json"),
        r#"{ "compilerOptions": { "paths": { "feature/*": ["../features/*"] } } }"#,
    )
    .expect("write referenced aliases");
    fs::write(site.join("entry.ts"), "import 'feature/report';").expect("write entry");
    fs::write(
        site.join("features/report.ts"),
        "export function report() {}",
    )
    .expect("write target");

    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let report = analyze(&config).expect("analyze project");
    let edge = report
        .file_graph
        .edges
        .first()
        .expect("resolve configured dependency");
    assert_eq!(edge.from, "site/entry.ts");
    assert_eq!(edge.to, "site/features/report.ts");
}

#[test]
fn standalone_graph_resolution_keeps_type_and_abstract_dependencies_distinct() {
    use archlens::parser::parse_source;
    use archlens::resolver::resolve_file_graph;
    use archlens::{ImportKind, Language};

    let temp = tempfile::TempDir::new().expect("temporary repository");
    let files = [
        ("data.ts", "export type Data = string;"),
        ("port.ts", "export interface Port { run(): void; }"),
        ("mixed.ts", "export interface Request { key: string }\nexport function send() {}"),
        ("entry.ts", "import type { Data } from './data';\nimport { Port } from './port';\nimport { Request, send } from './mixed';"),
    ]
    .map(|(path, source)| parse_source(path, source, Language::TypeScript, false).expect("parse fixture"));

    let graph = resolve_file_graph(temp.path(), &files, false, false);
    let edges: Vec<_> = graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.kind, edge.is_dip))
        .collect();
    assert_eq!(
        edges,
        [
            ("entry.ts", "data.ts", ImportKind::TypeOnly, true),
            ("entry.ts", "mixed.ts", ImportKind::Import, false),
            ("entry.ts", "port.ts", ImportKind::Import, true),
        ]
    );
}
