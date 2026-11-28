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
