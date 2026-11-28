use std::collections::BTreeSet;
use std::fs;

use archlens::AnalysisConfig;
use archlens::scanner::scan_repository;
use tempfile::TempDir;

#[test]
fn scanner_excludes_test_paths_only_in_the_default_scan() {
    let temp = TempDir::new().expect("temporary repository");
    let root = temp.path();
    let test_paths = [
        "tests/helpers.rs",
        "src/module_tests.rs",
        "src/module_test.rs",
        "src/TESTS.rs",
        "src/test_support.rs",
        "src/test_utils.rs",
        "src/widget.test.ts",
        "src/widget.test.tsx",
        "src/widget.test.js",
        "src/widget.test.jsx",
        "src/widget.spec.ts",
        "src/widget.spec.tsx",
        "src/widget.spec.js",
        "src/widget.spec.jsx",
        "src/widget.stories.ts",
        "src/widget.stories.tsx",
        "src/widget.stories.js",
        "src/module_test.go",
        "src/module_test.py",
        "src/test_module.py",
        "src/conftest.py",
    ];
    let production_paths = ["src/contests.rs", "src/testimony.ts", "src/spec.ts"];
    let all_paths = test_paths.into_iter().chain(production_paths);
    for path in all_paths.clone() {
        let file = root.join(path);
        fs::create_dir_all(file.parent().expect("file parent")).expect("create fixture folder");
        fs::write(file, "").expect("create fixture file");
    }

    let scan = |include_tests| {
        let mut config = AnalysisConfig::new(root);
        config.include_tests = include_tests;
        scan_repository(&config)
            .expect("scan fixture")
            .into_iter()
            .map(|file| file.rel_path)
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        scan(false),
        production_paths.into_iter().map(String::from).collect()
    );
    assert_eq!(scan(true), all_paths.map(String::from).collect());
}
