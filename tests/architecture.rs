use archlens::parser::parse_file;
use archlens::resolver::rust::resolve_rust_import_direct;
use archlens::resolver::{ProjectIndex, ResolveOutcome};
use archlens::scanner::scan_repository;
use archlens::{AnalysisConfig, analyze};

#[test]
fn domain_does_not_import_from_adapters() {
    let config = AnalysisConfig::new(env!("CARGO_MANIFEST_DIR"));
    let files: Vec<_> = scan_repository(&config)
        .expect("scan archlens sources")
        .iter()
        .map(|file| {
            parse_file(&file.abs_path, &file.rel_path, file.language, false)
                .expect("parse archlens sources")
        })
        .collect();
    let index = ProjectIndex::build(&config.root, &files);
    let mut violations = Vec::new();
    let mut resolved_imports = 0;

    for file in files
        .iter()
        .filter(|file| file.path.starts_with("src/domain/"))
    {
        for import in &file.raw_imports {
            // Public compatibility re-exports can hide a dependency on an adapter.
            if let ResolveOutcome::Internal(target) =
                resolve_rust_import_direct(&index, &file.path, import)
            {
                resolved_imports += 1;
                let module = target
                    .strip_prefix("src/")
                    .and_then(|path| path.split(['/', '.']).next());
                if matches!(
                    module,
                    Some("git" | "parser" | "report" | "resolver" | "scanner")
                ) {
                    violations.push(format!("{}:{} -> {target}", file.path, import.line));
                }
            }
        }
    }

    assert!(
        resolved_imports > 0,
        "self-analysis must resolve domain imports"
    );
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn top_level_modules_are_acyclic() {
    let mut config = AnalysisConfig::new(env!("CARGO_MANIFEST_DIR"));
    config.git.enabled = false;
    let report = analyze(&config).expect("analyze archlens sources");
    let modules = report
        .views
        .iter()
        .find(|view| view.id == "modules")
        .expect("modules view");

    assert!(
        !modules.edges.is_empty(),
        "self-analysis must resolve module edges"
    );
    assert!(modules.cycles.is_empty(), "{:#?}", modules.cycles);
}
