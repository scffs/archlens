#![no_main]

use std::collections::BTreeSet;
use std::fmt::Write;
use std::fs;

use archlens::{AnalysisConfig, analyze};
use libfuzzer_sys::fuzz_target;

fn source_path(file: usize) -> String {
    format!("src/file_{file}.ts")
}

fuzz_target!(|input: &[u8]| {
    let Some((&selector, choices)) = input.split_first() else {
        return;
    };
    let file_count = 1 + usize::from(selector % 6);
    let project = tempfile::tempdir().expect("create temporary project");
    let root = project.path();
    fs::create_dir(root.join("src")).expect("create source directory");
    fs::write(
        root.join("archlens.toml"),
        "exclude = ['src/excluded.ts']\n[git]\nenabled = false\n",
    )
    .expect("write Archlens config");
    fs::write(
        root.join("tsconfig.json"),
        r#"{
            // Use a project alias alongside relative imports.
            "compilerOptions": {
                "baseUrl": ".",
                "paths": {"@fixture/*": ["src/*"]},
            },
        }"#,
    )
    .expect("write TypeScript config");
    fs::write(root.join("src/excluded.ts"), "import './file_0';\n").expect("write excluded source");

    let mut expected_edges = BTreeSet::new();
    for from in 0..file_count {
        let mut source = String::new();
        let from_path = source_path(from);
        for to in 0..file_count {
            if from == to {
                continue;
            }
            let choice = choices.get(from * file_count + to).copied().unwrap_or(0);
            let specifier = match choice % 3 {
                1 => format!("./file_{to}"),
                2 => format!("@fixture/file_{to}"),
                _ => continue,
            };
            writeln!(source, "import '{specifier}';").expect("write import");
            expected_edges.insert((from_path.clone(), source_path(to)));
        }
        writeln!(source, "export const value = {from};").expect("write export");
        fs::write(root.join(&from_path), source).expect("write source file");
    }

    let config = AnalysisConfig::load_or_default(root).expect("load generated Archlens config");
    let report = analyze(&config).expect("analyze generated project");
    let edges: BTreeSet<_> = report
        .file_graph
        .edges
        .iter()
        .map(|edge| (edge.from.clone(), edge.to.clone()))
        .collect();
    assert_eq!(report.total_files, file_count);
    assert_eq!(report.total_internal_edges, expected_edges.len());
    assert_eq!(report.unresolved_import_count, 0);
    assert_eq!(edges, expected_edges);
});
