use std::fs;

use archlens::{AnalysisConfig, AnalysisReport, ArchitectureRule, LayerRule, RuleKind, analyze};

fn analyze_fixture(
    files: &[(&str, &str)],
    layers: Vec<LayerRule>,
    rules: Vec<ArchitectureRule>,
) -> AnalysisReport {
    let project = tempfile::tempdir().expect("create fixture directory");
    for (name, source) in files {
        let path = project.path().join(name);
        fs::create_dir_all(path.parent().expect("file has a directory")).expect("create directory");
        fs::write(path, source).expect("write fixture");
    }

    let mut config = AnalysisConfig::new(project.path());
    config.git.enabled = false;
    config.layers = layers;
    config.rules = rules;
    let report = analyze(&config).expect("analyze fixture");
    assert_eq!(report.total_files, files.len());
    assert_eq!(report.unresolved_import_count, 0);
    report
}

#[test]
fn boundary_rules_distinguish_allowed_facades_local_dependencies_and_type_imports() {
    let report = analyze_fixture(
        &[
            (
                "src/app/entry.ts",
                concat!(
                    "import './peer';\n",
                    "import '../infra/index';\n",
                    "import { value } from '../infra/private';\n",
                    "import type { Contract } from '../infra/types';\n",
                    "import '../foundation/tool';\n",
                ),
            ),
            ("src/app/peer.ts", "export const peer = true;\n"),
            ("src/infra/index.ts", "export { value } from './private';\n"),
            ("src/infra/private.ts", "export const value = 1;\n"),
            ("src/infra/types.ts", "export interface Contract {}\n"),
            ("src/foundation/tool.ts", "export const tool = true;\n"),
            (
                "src/other/entry.ts",
                "import '../infra/index';\nimport '../app/peer';\n",
            ),
        ],
        vec![],
        vec![
            ArchitectureRule {
                name: "infra-facade".into(),
                kind: RuleKind::PublicApi,
                from: vec!["src/infra/**".into()],
                to: vec!["src/infra/index.ts".into()],
                allow_dip: true,
                reason: Some("Import infrastructure through its public entry point".into()),
            },
            ArchitectureRule {
                name: "forbid-app-private".into(),
                kind: RuleKind::Forbid,
                from: vec!["src/app/**".into()],
                to: vec!["src/infra/private.ts".into(), "src/infra/types.ts".into()],
                allow_dip: true,
                reason: None,
            },
            ArchitectureRule {
                name: "allow-app-infra".into(),
                kind: RuleKind::AllowOnly,
                from: vec!["src/app/**".into()],
                to: vec!["src/infra/index.ts".into()],
                allow_dip: true,
                reason: None,
            },
        ],
    );

    assert_eq!(report.total_internal_edges, 8);
    assert!(report.file_graph.edges.iter().any(|edge| {
        edge.from == "src/app/entry.ts" && edge.to == "src/infra/types.ts" && edge.is_dip
    }));
    let violations = report.rule_violations;
    let matches: Vec<_> = violations
        .iter()
        .map(|v| {
            (
                v.rule_name.as_str(),
                v.kind.as_str(),
                v.from_file.as_str(),
                v.to_file.as_str(),
                v.line,
            )
        })
        .collect();
    let app = "src/app/entry.ts";
    let private = "src/infra/private.ts";
    assert_eq!(
        matches,
        [
            (
                "allow-app-infra",
                "allow_only",
                app,
                "src/foundation/tool.ts",
                5,
            ),
            ("allow-app-infra", "allow_only", app, private, 3),
            ("forbid-app-private", "forbid", app, private, 3),
            ("infra-facade", "public_api", app, private, 3),
        ],
    );
    let facade = &violations[3];
    assert_eq!(facade.from_module, "app");
    assert_eq!(facade.to_module, "infra");
    assert_eq!(facade.symbols, ["value"]);
    assert_eq!(
        facade.message,
        "Import infrastructure through its public entry point"
    );
}

#[test]
fn layer_order_allows_inward_same_layer_and_explicit_dip_dependencies() {
    let report = analyze_fixture(
        &[
            ("src/outer/service.ts", "import '../inner/core';\n"),
            ("src/outer/types.ts", "export interface Contract {}\n"),
            (
                "src/inner/core.ts",
                concat!(
                    "import './peer';\n",
                    "import type { Contract } from '../outer/types';\n",
                    "import '../outer/service';\n",
                ),
            ),
            ("src/inner/peer.ts", "export const peer = true;\n"),
        ],
        vec![
            LayerRule {
                name: "outer".into(),
                patterns: vec!["src/outer/**".into()],
                allow_dip: false,
            },
            LayerRule {
                name: "inner".into(),
                patterns: vec!["src/inner/**".into()],
                allow_dip: true,
            },
        ],
        vec![],
    );

    assert_eq!(report.total_internal_edges, 4);
    assert!(report.file_graph.edges.iter().any(|edge| {
        edge.from == "src/inner/core.ts" && edge.to == "src/outer/types.ts" && edge.is_dip
    }));
    let violations = report.rule_violations;
    assert_eq!(violations.len(), 1);
    let violation = &violations[0];
    assert_eq!(violation.rule_name, "layer:inner->outer");
    assert_eq!(violation.from_file, "src/inner/core.ts");
    assert_eq!(violation.to_file, "src/outer/service.ts");
    assert_eq!(violation.line, 3);
}
