use archlens::parser::parse_file;
use archlens::resolver::rust::resolve_rust_import_direct;
use archlens::resolver::{ProjectIndex, ResolveOutcome};
use archlens::scanner::scan_repository;
use archlens::{AnalysisConfig, Language, analyze};

#[test]
fn imports_respect_adapter_boundaries() {
    let mut config = AnalysisConfig::new(env!("CARGO_MANIFEST_DIR"));
    config.languages.push(Language::Rust);
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

    for file in &files {
        for import in &file.raw_imports {
            // Public compatibility re-exports can hide a dependency on an adapter.
            if let ResolveOutcome::Internal(target) =
                resolve_rust_import_direct(&index, &file.path, import)
            {
                resolved_imports += usize::from(target != file.path);
                let module = target
                    .strip_prefix("src/")
                    .and_then(|path| path.split(['/', '.']).next());
                let domain_imports_adapter = file.path.starts_with("src/domain/")
                    && matches!(
                        module,
                        Some("git" | "parser" | "plugins" | "report" | "resolver" | "scanner")
                    );
                let adapter_imports_dispatcher = [
                    ("src/parser/", "src/parser/mod.rs"),
                    ("src/resolver/", "src/resolver/mod.rs"),
                ]
                .iter()
                .any(|(dir, dispatcher)| {
                    file.path.starts_with(dir) && file.path != *dispatcher && target == *dispatcher
                });

                let parser_imports_other_stage = file.path.starts_with("src/parser/")
                    && matches!(module, Some("plugins" | "resolver" | "scanner"));
                let resolver_imports_other_stage = file.path.starts_with("src/resolver/")
                    && matches!(module, Some("parser" | "plugins" | "scanner"));

                let generic_imports_framework = file.path == "src/resolver/js_ts/imports.rs"
                    && matches!(
                        target.as_str(),
                        "src/resolver/nuxt.rs"
                            | "src/resolver/web_defaults.rs"
                            | "src/resolver/js_ts.rs"
                    );

                if domain_imports_adapter
                    || adapter_imports_dispatcher
                    || parser_imports_other_stage
                    || resolver_imports_other_stage
                    || generic_imports_framework
                {
                    violations.push(format!("{}:{} -> {target}", file.path, import.line));
                }
            }
        }
    }

    assert!(
        resolved_imports > 0,
        "self-analysis must resolve source imports"
    );
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn module_and_file_dependencies_are_acyclic() {
    let mut config = AnalysisConfig::new(env!("CARGO_MANIFEST_DIR"));
    config.git.enabled = false;
    let report = analyze(&config).expect("analyze archlens sources");

    for view_id in ["modules", "files"] {
        let view = report
            .views
            .iter()
            .find(|view| view.id == view_id)
            .expect("view");
        assert!(
            !view.edges.is_empty(),
            "self-analysis must resolve {view_id} edges"
        );
        assert!(view.cycles.is_empty(), "{view_id}: {:#?}", view.cycles);
    }
}
