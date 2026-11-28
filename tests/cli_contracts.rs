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

#[test]
fn diff_gate_detects_new_cycles_and_rule_violations_independently() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    fs::write(root.join("a.ts"), "import './b';\n").expect("write a.ts");
    fs::write(root.join("b.ts"), "export const b = 1;\n").expect("write b.ts");

    let snapshot = |name| {
        let path = root.join(name);
        let output = Command::new(env!("CARGO_BIN_EXE_archlens"))
            .arg("analyze")
            .arg(root)
            .args(["--no-git", "--quiet", "--json"])
            .arg(&path)
            .output()
            .expect("write snapshot");
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        path
    };

    let baseline = snapshot("base.json");
    fs::write(root.join("b.ts"), "import './a';\n").expect("introduce cycle");
    let with_cycle = snapshot("cycle.json");
    fs::write(root.join("b.ts"), "export const b = 1;\n").expect("remove cycle");
    fs::write(
        root.join("archlens.toml"),
        "[[rules]]\nname = 'a-must-not-import-b'\nkind = 'forbid'\nfrom = ['a.ts']\nto = ['b.ts']\n",
    )
    .expect("add rule");
    let with_rule_violation = snapshot("rule.json");

    let diff = |current: &std::path::Path, enforce: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_archlens"));
        command
            .arg("diff")
            .arg(&baseline)
            .arg(current)
            .args(["--view", "files"]);
        if enforce {
            command.arg("--fail-on-regression");
        }
        command.output().expect("run archlens diff")
    };

    let unchanged = diff(&baseline, true);
    assert!(unchanged.status.success(), "{unchanged:?}");
    assert_eq!(
        diff(&root.join("missing.json"), true).status.code(),
        Some(2)
    );
    for (current, expected, absent) in [
        (&with_cycle, "NEW cycles (1)", "NEW rule violations"),
        (&with_rule_violation, "[a-must-not-import-b]", "NEW cycles"),
    ] {
        let enforced = diff(current, true);
        assert_eq!(enforced.status.code(), Some(1), "{enforced:?}");
        let stdout = String::from_utf8_lossy(&enforced.stdout);
        assert!(stdout.contains("Baseline Diff (view: files)"), "{stdout}");
        assert!(stdout.contains(expected), "{stdout}");
        assert!(!stdout.contains(absent), "{stdout}");
        let advisory = diff(current, false);
        assert!(advisory.status.success(), "{advisory:?}");
        assert_eq!(advisory.stdout, enforced.stdout);
    }
}

#[test]
fn init_reports_a_missing_project_as_an_io_error() {
    let temp = TempDir::new().expect("tempdir");
    let missing = temp.path().join("missing");

    let output = Command::new(env!("CARGO_BIN_EXE_archlens"))
        .arg("init")
        .arg(&missing)
        .output()
        .expect("run archlens init");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"), "{stderr}");
    assert!(stderr.contains("archlens.toml"), "{stderr}");
    assert!(!missing.exists());
}
