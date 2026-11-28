use std::fs;
use std::path::Path;

use archlens::config::{
    AnalysisConfig, ArchitectureRule, ComponentRule, LayerRule, RuleKind, ThresholdConfig,
};
use archlens::diff::compute_baseline_diff;
use archlens::domain::metrics::ArchitectureZone;
use archlens::domain::model::ImportKind;
use archlens::report::html::render_html_report;
use archlens::report::json::to_json_string;
use archlens::{analyze, parser};
use tempfile::TempDir;

fn write_fixture(root: &Path, rel: &str, content: &str) {
    let full = root.join(rel);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).expect("create fixture parent dirs");
    }
    fs::write(full, content).expect("write fixture file");
}

#[test]
fn computes_clean_architecture_metrics_for_rust_layers() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();

    write_fixture(
        root,
        "Cargo.toml",
        r#"[package]
name = "demo_app"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_fixture(
        root,
        "src/lib.rs",
        "pub mod domain;\npub mod infra;\npub mod app;\npub mod unused_abs;\n",
    );
    write_fixture(root, "src/domain/mod.rs", "pub mod ports;\n");
    write_fixture(
        root,
        "src/domain/ports.rs",
        "pub trait OrderRepo { fn save(&self); }\npub trait Clock { fn now(&self) -> u64; }\n",
    );
    write_fixture(root, "src/infra/mod.rs", "pub mod pg;\n");
    write_fixture(
        root,
        "src/infra/pg.rs",
        "use crate::domain::ports::OrderRepo;\npub struct PgRepo;\nimpl OrderRepo for PgRepo { fn save(&self) {} }\n",
    );
    write_fixture(root, "src/app/mod.rs", "pub mod service;\n");
    write_fixture(
        root,
        "src/app/service.rs",
        r"use crate::domain::ports::{Clock, OrderRepo};
use crate::infra::pg::PgRepo;

pub struct AppService;
pub fn run() {}

#[cfg(test)]
mod tests {
    use super::*;
    pub struct TestOnlyHelper;
}
",
    );
    write_fixture(
        root,
        "src/unused_abs/mod.rs",
        "use crate::domain::ports::OrderRepo;\npub trait UnusedPort { fn ping(&self); }\n",
    );

    let mut cfg = AnalysisConfig::new(root);
    cfg.git.enabled = false;
    cfg.components = vec![
        ComponentRule {
            name: "domain".to_string(),
            patterns: vec!["src/domain/**".to_string()],
        },
        ComponentRule {
            name: "infra".to_string(),
            patterns: vec!["src/infra/**".to_string()],
        },
        ComponentRule {
            name: "app".to_string(),
            patterns: vec!["src/app/**".to_string()],
        },
        ComponentRule {
            name: "unused_abs".to_string(),
            patterns: vec!["src/unused_abs/**".to_string()],
        },
    ];

    let report = analyze(&cfg).expect("analyze");
    let view = report
        .views
        .iter()
        .find(|v| v.id == "components")
        .expect("components view");

    let domain = view.nodes.iter().find(|n| n.id == "domain").expect("domain");
    assert_eq!(domain.abstractness, Some(1.0));
    assert_eq!(domain.instability, Some(0.0));
    assert_eq!(domain.distance, Some(0.0));
    assert_eq!(domain.zone, ArchitectureZone::MainSequence);
    assert!(domain.afferent_coupling >= 3);

    // `infra/pg.rs` implements `OrderRepo` trait -> must be tagged as DIP (`TraitImpl`).
    let dip_edge = report
        .file_graph
        .edges
        .iter()
        .find(|e| e.from.ends_with("pg.rs") && e.to.ends_with("ports.rs"))
        .expect("pg -> ports edge");
    assert_eq!(dip_edge.kind, ImportKind::TraitImpl);
    assert!(dip_edge.is_dip);
    assert!(dip_edge.symbols.contains(&"OrderRepo".to_string()));

    let app = view.nodes.iter().find(|n| n.id == "app").expect("app");
    assert_eq!(app.symbols.concrete_declarations, 2);
    assert_eq!(app.abstractness, Some(0.0));
    assert_eq!(app.instability, Some(1.0));
    assert_eq!(app.zone, ArchitectureZone::MainSequence);

    let unused = view
        .nodes
        .iter()
        .find(|n| n.id == "unused_abs")
        .expect("unused_abs");
    assert_eq!(unused.afferent_coupling, 0);
    assert_eq!(unused.abstractness, Some(1.0));
    assert_eq!(unused.instability, Some(1.0));
    assert_eq!(unused.distance, Some(1.0));
    assert_eq!(unused.zone, ArchitectureZone::ZoneOfUselessness);

    let infra = view.nodes.iter().find(|n| n.id == "infra").expect("infra");
    assert_eq!(infra.abstractness, Some(0.0));
    assert_eq!(infra.instability, Some(0.5));
    assert_eq!(infra.zone, ArchitectureZone::ZoneOfPain);

    assert!(view.cycles.is_empty());
    assert!(view.summary.propagation_cost > 0.0);
    assert!(view.summary.nccd > 0.0);

    let pos = |id: &str| view.dsm_order.iter().position(|x| x == id).expect("in dsm");
    assert!(pos("domain") < pos("infra"));
    assert!(pos("infra") < pos("app"));
}

#[test]
fn excludes_cfg_test_external_modules_and_pairs_rust_2018_roots() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();

    write_fixture(
        root,
        "Cargo.toml",
        "[package]\nname = \"svc\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    write_fixture(
        root,
        "src/lib.rs",
        "pub mod application;\npub mod infra;\npub mod di;\n",
    );
    // Rust 2018 module root paired with `src/application/` plus external `#[cfg(test)] mod custom_spec;`
    write_fixture(
        root,
        "src/application.rs",
        "pub mod auth;\n#[cfg(test)]\nmod custom_spec;\n",
    );
    write_fixture(
        root,
        "src/application/auth.rs",
        "pub trait AuthPort { fn ok(&self) -> bool; }\nfn private_helper() {}\n",
    );
    write_fixture(
        root,
        "src/application/custom_spec.rs",
        "use crate::infra::adapter::Adapter;\npub fn test_everything() { let _ = Adapter; }\n",
    );
    write_fixture(
        root,
        "src/application/auth_tests.rs",
        "use crate::di::wire;\npub fn unit_test() { wire(); }\n",
    );
    write_fixture(root, "src/infra.rs", "pub mod adapter;\n");
    write_fixture(
        root,
        "src/infra/adapter.rs",
        "use crate::application::auth::AuthPort;\npub struct Adapter;\nimpl AuthPort for Adapter { fn ok(&self) -> bool { true } }\n",
    );
    write_fixture(
        root,
        "src/di.rs",
        "use crate::infra::adapter::Adapter;\npub fn wire() { let _ = Adapter; }\n",
    );

    let mut cfg = AnalysisConfig::new(root);
    cfg.git.enabled = false;
    let report = analyze(&cfg).expect("analyze rust 2018");

    // Both `custom_spec.rs` (via `#[cfg(test)] mod`) and `auth_tests.rs` must be excluded.
    assert!(
        !report
            .file_graph
            .files
            .iter()
            .any(|f| f.path.contains("custom_spec") || f.path.contains("auth_tests"))
    );

    let modules = report
        .views
        .iter()
        .find(|v| v.id == "modules")
        .expect("modules");

    // No artificial `src` bucket and zero cycles!
    assert!(!modules.nodes.iter().any(|n| n.id == "src"));
    assert!(modules.cycles.is_empty());

    // Private helper in `auth.rs` must not dilute public `trait AuthPort` -> A = 1.0!
    let app_mod = modules
        .nodes
        .iter()
        .find(|n| n.id == "application")
        .expect("application");
    assert_eq!(app_mod.abstractness, Some(1.0));
}

#[test]
fn evaluates_architecture_rules_layers_cycle_breaks_and_baseline_diff() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();

    write_fixture(root, "package.json", r#"{ "name": "shop", "private": true }"#);
    write_fixture(
        root,
        "src/domain/order.ts",
        "import { dbQuery } from '../infra/internal';\nexport interface Order { id: string }\nexport function load() { return dbQuery(); }\n",
    );
    write_fixture(
        root,
        "src/infra/index.ts",
        "export { dbQuery } from './internal';\n",
    );
    write_fixture(
        root,
        "src/infra/internal.ts",
        "import type { Order } from '../domain/order';\nexport function dbQuery(): Order { return { id: '1' }; }\n",
    );

    let mut cfg = AnalysisConfig::new(root);
    cfg.git.enabled = false;
    cfg.layers = vec![
        LayerRule {
            name: "infra".to_string(),
            patterns: vec!["src/infra/**".to_string()],
            allow_dip: false,
        },
        LayerRule {
            name: "domain".to_string(),
            patterns: vec!["src/domain/**".to_string()],
            allow_dip: false,
        },
    ];
    cfg.rules = vec![ArchitectureRule {
        name: "infra-public-facade".to_string(),
        kind: RuleKind::PublicApi,
        from: vec!["src/infra/**".to_string()],
        to: vec!["src/infra/index.ts".to_string()],
        allow_dip: false,
        reason: Some("import infra via index.ts only".to_string()),
    }];
    cfg.thresholds = ThresholdConfig {
        fail_on_cycles: true,
        fail_on_rule_violations: true,
        ..ThresholdConfig::default()
    };

    let before = analyze(&cfg).expect("before");
    let modules = before.views.iter().find(|v| v.id == "modules").unwrap();
    assert_eq!(modules.cycles.len(), 1);
    assert!(!modules.cycles[0].break_suggestions.is_empty());

    // Must catch both layer violation (domain -> infra) and public_api facade bypass!
    assert!(before.rule_violations.iter().any(|r| r.kind == "layer"));
    assert!(
        before
            .rule_violations
            .iter()
            .any(|r| r.rule_name == "infra-public-facade")
    );
    assert!(!before.check_violations.is_empty());

    // Fix the forbidden import and verify BaselineDiff detects the resolved cycle & rule fix.
    write_fixture(
        root,
        "src/domain/order.ts",
        "export interface Order { id: string }\n",
    );
    let after = analyze(&cfg).expect("after");
    let diff = compute_baseline_diff(&before, &after, Some("modules")).expect("diff");
    assert_eq!(diff.delta_cycles, -1);
    assert_eq!(diff.resolved_cycles.len(), 1);
    assert_eq!(diff.new_cycles.len(), 0);
    assert!(diff.delta_rule_violations < 0);
}

#[test]
fn detects_ts_vue_tsconfig_aliases_cycles_and_sdp() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();

    write_fixture(
        root,
        "package.json",
        r#"{ "name": "web-app", "private": true }"#,
    );
    write_fixture(
        root,
        "tsconfig.json",
        r#"{
  // JSONC comment and trailing comma must parse cleanly
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"],
    },
  },
}"#,
    );
    write_fixture(
        root,
        "src/core/types.ts",
        r"export interface Quote { ticker: string; price: number; }
export type QuoteFetcher = (ticker: string) => Promise<Quote>;
",
    );
    write_fixture(
        root,
        "src/feature_a/index.ts",
        r"import { bHelper } from '@/feature_b/helper';
import type { Quote } from '@/core/types';
export function aRunner(q: Quote) { return bHelper(q); }
",
    );
    write_fixture(
        root,
        "src/feature_b/helper.ts",
        r"import { aRunner } from '../feature_a';
import type { Quote } from '@/core/types';
export function bHelper(q: Quote) { return aRunner(q); }
",
    );
    write_fixture(
        root,
        "src/pages/Dashboard.vue",
        r#"<script setup lang="ts">
import { aRunner } from '@/feature_a';
import type { Quote } from '@/core/types';
const sample: Quote = { ticker: 'SBER', price: 300 };
aRunner(sample);
</script>
<template><div>Dashboard</div></template>
"#,
    );

    let mut cfg = AnalysisConfig::new(root);
    cfg.git.enabled = false;
    cfg.thresholds = ThresholdConfig {
        fail_on_cycles: true,
        ..ThresholdConfig::default()
    };

    let report = analyze(&cfg).expect("analyze ts/vue");
    assert_eq!(report.total_files, 4);
    assert_eq!(report.unresolved_import_count, 0);

    let modules = report
        .views
        .iter()
        .find(|v| v.id == "modules")
        .expect("modules view");

    assert_eq!(modules.cycles.len(), 1);
    assert_eq!(modules.cycles[0].nodes, vec!["feature_a", "feature_b"]);
    assert!(
        report
            .check_violations
            .iter()
            .any(|v| v.rule == "fail_on_cycles")
    );

    let core = modules.nodes.iter().find(|n| n.id == "core").expect("core");
    assert_eq!(core.abstractness, Some(1.0));
    assert_eq!(core.instability, Some(0.0));
    assert_eq!(core.distance, Some(0.0));

    let pages = modules.nodes.iter().find(|n| n.id == "pages").expect("pages");
    assert_eq!(pages.component_fan_out, 2);
    assert_eq!(pages.instability, Some(1.0));

    let json = to_json_string(&report).expect("json");
    assert!(json.contains("feature_a"));
    let html = render_html_report(&report).expect("html");
    assert!(html.contains("Граф зависимостей"));
    assert!(html.contains("Матрица связей"));
    assert!(html.contains("Dashboard.vue"));
}

#[test]
fn parses_and_resolves_python_and_go_projects() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();

    write_fixture(
        root,
        "pypkg/pyproject.toml",
        "[project]\nname = \"pypkg\"\n",
    );
    write_fixture(root, "pypkg/src/pypkg/__init__.py", "");
    write_fixture(
        root,
        "pypkg/src/pypkg/ports.py",
        r"from typing import Protocol
class Repo(Protocol):
    def get(self, key: str) -> str: ...
",
    );
    write_fixture(
        root,
        "pypkg/src/pypkg/service.py",
        r#"from .ports import Repo
class Service:
    def run(self, r: Repo) -> str:
        return r.get("x")
"#,
    );

    write_fixture(root, "gomod/go.mod", "module example.com/demo\n\ngo 1.23\n");
    write_fixture(
        root,
        "gomod/domain/repo.go",
        "package domain\ntype Store interface { Get(id string) string }\n",
    );
    write_fixture(
        root,
        "gomod/app/main.go",
        "package main\nimport \"example.com/demo/domain\"\ntype Runner struct{}\nfunc main() { var _ domain.Store }\n",
    );

    let mut cfg = AnalysisConfig::new(root);
    cfg.git.enabled = false;
    let report = analyze(&cfg).expect("analyze py+go");
    assert_eq!(report.unresolved_import_count, 0);

    let files_view = report
        .views
        .iter()
        .find(|v| v.id == "files")
        .expect("files view");

    assert!(
        files_view
            .edges
            .iter()
            .any(|e| e.from.ends_with("service.py") && e.to.ends_with("ports.py"))
    );
    assert!(
        files_view
            .edges
            .iter()
            .any(|e| e.from.ends_with("app/main.go") && e.to.ends_with("domain/repo.go"))
    );

    let ports = files_view
        .nodes
        .iter()
        .find(|n| n.id.ends_with("ports.py"))
        .expect("ports.py");
    assert_eq!(ports.abstractness, Some(1.0));

    let go_domain = files_view
        .nodes
        .iter()
        .find(|n| n.id.ends_with("domain/repo.go"))
        .expect("repo.go");
    assert_eq!(go_domain.abstractness, Some(1.0));
}

#[test]
fn html_escapes_script_closing_sequences() {
    let parsed = parser::parse_source(
        "src/app.ts",
        "export interface Foo { x: number; }\n",
        archlens::Language::TypeScript,
        false,
    )
    .expect("parse");
    assert_eq!(parsed.symbols.abstract_declarations, 1);

    let tmp = TempDir::new().expect("tempdir");
    write_fixture(tmp.path(), "src/a.ts", "export function hello() {}\n");
    let mut cfg = AnalysisConfig::new(tmp.path());
    cfg.git.enabled = false;
    let mut report = analyze(&cfg).expect("analyze");
    report.project_name = "bad</script><script>alert(1)".to_string();

    let html = render_html_report(&report).expect("html");
    assert!(!html.contains("bad</script>"));
    assert!(html.contains("bad<\\/script>"));
}

#[test]
fn resolves_unicode_tsconfig_aliases_with_comments_and_trailing_commas() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    write_fixture(
        root,
        "tsconfig.json",
        r#"{
  // Alias and directory names are meaningful Unicode text.
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@данные/*": ["src/модели/*",],
    },
  },
}"#,
    );
    write_fixture(
        root,
        "src/app.ts",
        "import { item } from '@данные/item';\nexport const entry = item;\n",
    );
    write_fixture(root, "src/модели/item.ts", "export const item = 1;\n");
    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;

    let report = analyze(&config).expect("analyze TypeScript fixture");
    assert!(
        report.file_graph.edges.iter().any(|edge| {
            edge.from == "src/app.ts" && edge.to == "src/модели/item.ts"
        }),
        "the Unicode alias should resolve to the original path"
    );
}
