use std::fs;

use archlens::domain::model::{ParsedFile, RawImport};
use archlens::parser::{LanguageParser, parse_source};
use archlens::{
    AnalysisConfig, AnalysisPlugins, ImportKind, Language, Result, analyze, analyze_with_plugins,
};
use tempfile::TempDir;

struct RuleParser;

impl LanguageParser for RuleParser {
    fn parse(&self, path: &str, source: &str, include_tests: bool) -> Result<ParsedFile> {
        let mut parsed = parse_source(path, source, Language::TypeScript, include_tests)?;
        parsed.path = "parser-supplied-name.ts".into();
        parsed.language = Language::Go;
        parsed.raw_imports.push(RawImport {
            specifier: "./target.ts".into(),
            kind: ImportKind::Import,
            line: 1,
            symbols: Vec::new(),
        });
        Ok(parsed)
    }
}

#[test]
fn registered_extension_and_parser_work_through_the_analysis_entry_point() {
    let temp = TempDir::new().expect("tempdir");
    fs::write(temp.path().join("entry.RULE"), "// dependency rule\n").expect("write rule");
    fs::write(
        temp.path().join("target.ts"),
        "export function target() { return 1; }\n",
    )
    .expect("write TypeScript");
    let mut config = AnalysisConfig::new(temp.path());
    config.git.enabled = false;

    let baseline = analyze(&config).expect("analyze with defaults");
    assert_eq!(baseline.total_files, 1);
    assert!(baseline.file_graph.edges.is_empty());

    let mut plugins = AnalysisPlugins::default();
    plugins.register_source(".rule", Language::TypeScript, &RuleParser);
    let report = analyze_with_plugins(&config, &plugins).expect("analyze with a plugin");
    assert_eq!(report.total_files, 2);
    let edge = report
        .file_graph
        .edges
        .first()
        .expect("edge from new parser");
    assert_eq!(edge.from, "entry.RULE");
    assert_eq!(edge.to, "target.ts");
    let target = report
        .file_graph
        .files
        .iter()
        .find(|file| file.path == "target.ts")
        .expect("built-in TypeScript");
    assert!(
        target
            .declarations
            .iter()
            .any(|symbol| symbol.name == "target")
    );
    assert!(target.unresolved_imports.is_empty());
}

struct FacadeResolver;

impl archlens::resolver::ImportResolver for FacadeResolver {
    fn resolve(
        &self,
        index: &archlens::resolver::ProjectIndex,
        _from: &str,
        import: &RawImport,
    ) -> archlens::resolver::ResolveOutcome {
        use archlens::resolver::ResolveOutcome;
        match import.specifier.as_str() {
            "fixture:target" => ResolveOutcome::Internal("target.ts".into()),
            "fixture:missing" => ResolveOutcome::Internal("missing.ts".into()),
            "fixture:facade" => index
                .reexports
                .get(&("facade.ts".into(), "value".into()))
                .map_or_else(
                    || ResolveOutcome::Unresolved(import.specifier.clone()),
                    |target| ResolveOutcome::Internal(target.clone()),
                ),
            _ => ResolveOutcome::External(import.specifier.clone()),
        }
    }
}

#[test]
fn registered_resolver_builds_and_uses_the_reexport_index() {
    let temp = TempDir::new().expect("tempdir");
    for (path, source) in [
        ("facade.ts", "export { value } from 'fixture:target';"),
        (
            "entry.ts",
            "import { value } from 'fixture:facade'; import 'fixture:missing';",
        ),
        ("target.ts", "export function value() {}"),
    ] {
        fs::write(temp.path().join(path), source).expect("write source");
    }

    let mut config = AnalysisConfig::new(temp.path());
    config.git.enabled = false;
    let mut plugins = AnalysisPlugins::default();
    plugins.register_source_resolver("ts", &FacadeResolver);
    let report = analyze_with_plugins(&config, &plugins).expect("analyze with custom resolution");

    assert_eq!(report.unresolved_import_count, 1);

    struct EmptyResolver;
    impl archlens::resolver::ImportResolver for EmptyResolver {
        fn resolve(
            &self,
            _index: &archlens::resolver::ProjectIndex,
            _from: &str,
            _import: &RawImport,
        ) -> archlens::resolver::ResolveOutcome {
            archlens::resolver::ResolveOutcome::External(String::new())
        }
    }

    let parser = archlens::parser::js_ts::JsTsParser::typescript(false);
    plugins.register_source("ts", Language::Go, &RuleParser);
    plugins.register_source_resolver("ts", &EmptyResolver);
    plugins.register_language(Language::TypeScript, &["ts"], &parser, &FacadeResolver);
    let reconfigured =
        analyze_with_plugins(&config, &plugins).expect("replace extension overrides");
    assert_eq!(reconfigured.file_graph.edges, report.file_graph.edges);

    let edges: Vec<_> = report
        .file_graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    assert_eq!(
        edges,
        [("entry.ts", "target.ts"), ("facade.ts", "target.ts")]
    );
}

#[test]
fn additional_views_work_with_scanned_and_prebuilt_graphs() {
    use archlens::domain::projection::ViewProjection;
    use archlens::{FileGraph, ViewProvider, analyze_file_graph_with_plugins};

    struct FixtureView(&'static str);
    impl ViewProvider for FixtureView {
        fn project(
            &self,
            _graph: &FileGraph,
            _config: &AnalysisConfig,
        ) -> Result<Option<ViewProjection>> {
            Ok(Some(ViewProjection {
                id: self.0.into(),
                title: "Fixture grouping".into(),
                description: "Group the target into a logical component.".into(),
                file_to_group: [("target.ts".into(), "component".into())].into(),
            }))
        }
    }

    let temp = TempDir::new().expect("tempdir");
    fs::write(temp.path().join("entry.ts"), "import './target';").expect("write importer");
    fs::write(temp.path().join("target.ts"), "export function target() {}").expect("write target");
    let mut config = AnalysisConfig::new(temp.path());
    config.git.enabled = false;

    let mut plugins = AnalysisPlugins::default();
    plugins.register_view(&FixtureView("modules"));
    plugins.register_view(&FixtureView("components"));
    plugins.register_view(&FixtureView("folders-d9"));
    plugins.register_view(&FixtureView("constructor"));
    plugins.register_view(&FixtureView("custom-view"));
    plugins.register_view(&FixtureView("custom-view"));
    let report = analyze_with_plugins(&config, &plugins).expect("analyze with view");
    assert_eq!(report.default_view, "modules");
    assert!(report.view("components").is_none());
    assert!(report.view("folders-d9").is_none());
    assert_eq!(
        report.view("constructor").unwrap().title,
        "Fixture grouping"
    );
    assert_ne!(report.view("modules").unwrap().title, "Fixture grouping");
    assert_eq!(
        report
            .views
            .iter()
            .filter(|view| view.id == "custom-view")
            .count(),
        1
    );
    let custom = report.view("custom-view").expect("custom view");
    assert_eq!(custom.edges.len(), 1);
    assert_eq!(custom.edges[0].from, "entry.ts");
    assert_eq!(custom.edges[0].to, "component");

    let prebuilt =
        analyze_file_graph_with_plugins(temp.path(), &report.file_graph, &config, &plugins)
            .expect("analyze prebuilt graph with view");
    assert!(prebuilt.view("custom-view").is_some());
}
