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

#[test]
fn init_creates_a_readable_config_and_keeps_an_existing_file() {
    let temp = TempDir::new().expect("tempdir");
    let init = || {
        Command::new(env!("CARGO_BIN_EXE_archlens"))
            .arg("init")
            .arg(temp.path())
            .output()
            .expect("run archlens init")
    };
    assert!(init().status.success());
    let path = temp.path().join("archlens.toml");
    archlens::AnalysisConfig::from_toml_file(&path, temp.path()).expect("read starter config");

    fs::write(&path, "# local settings\n").expect("write local config");
    assert_eq!(init().status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(&path).expect("read local config"),
        "# local settings\n"
    );
    fs::remove_file(&path).expect("remove temporary config");
    fs::create_dir(&path).expect("occupy config path with a directory");
    assert_eq!(init().status.code(), Some(1));
    assert!(path.is_dir());
}

#[cfg(unix)]
#[test]
fn init_treats_an_existing_dangling_link_as_occupied() {
    let temp = TempDir::new().expect("tempdir");
    let config_path = temp.path().join("archlens.toml");
    let link_target = temp.path().join("missing.toml");
    std::os::unix::fs::symlink(&link_target, &config_path).expect("create temporary symlink");

    let output = Command::new(env!("CARGO_BIN_EXE_archlens"))
        .arg("init")
        .arg(temp.path())
        .output()
        .expect("run archlens init");

    assert_eq!(output.status.code(), Some(1));
    assert!(!link_target.exists());
    assert_eq!(fs::read_link(config_path).expect("read link"), link_target);
}
