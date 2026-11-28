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
