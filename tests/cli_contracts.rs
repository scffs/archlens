use std::fs;
use std::process::Command;

use tempfile::TempDir;

#[test]
fn analyze_checks_the_requested_view_and_warns_about_unknown_views() {
    let temp = TempDir::new().expect("tempdir");
    let src = temp.path().join("src");
    fs::create_dir(&src).expect("create source directory");
    fs::write(src.join("a.ts"), "import './b';\n").expect("write a.ts");
    fs::write(src.join("b.ts"), "import './a';\n").expect("write b.ts");

    let analyze = |view| {
        Command::new(env!("CARGO_BIN_EXE_archlens"))
            .arg("analyze")
            .arg(temp.path())
            .args(["--view", view, "--fail-on-cycles", "--no-git", "--quiet"])
            .output()
            .expect("run archlens")
    };

    assert!(analyze("modules").status.success());
    assert_eq!(analyze("files").status.code(), Some(1));
    let unknown = analyze("unknown");
    assert!(unknown.status.success());
    let warning = String::from_utf8_lossy(&unknown.stderr);
    assert!(warning.contains("view 'unknown' not found"), "{warning}");
}
