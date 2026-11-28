use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use archlens::{AnalysisConfig, analyze};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) {
    git_with_env(root, args, &[]);
}

fn git_with_env(root: &Path, args: &[&str], env: &[(&str, &str)]) {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join(".git/nonexistent-config"))
        .envs(env.iter().copied())
        .arg("--literal-pathspecs")
        .args([
            "-c",
            "user.name=Alice",
            "-c",
            "user.email=alice@example.invalid",
        ])
        .args(args)
        .output()
        .expect("run Git");
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn temporal_coupling_limit_uses_a_stable_order_for_tied_pairs() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    git(root, &["init", "--quiet"]);

    let subproject = root.join("workspace[1]");
    fs::create_dir(&subproject).expect("create subproject");
    let names: Vec<_> = (0..14)
        .map(|index| format!("module_{index:02}.ts"))
        .collect();
    for version in 0..4 {
        for name in &names {
            fs::write(
                subproject.join(name),
                format!("export const version = {version};\n"),
            )
            .expect("write source");
        }
        git(root, &["add", "--", "workspace[1]"]);
        let author = if version < 2 {
            "Alice <alice@example.invalid>"
        } else {
            "Bob <bob@example.invalid>"
        };
        git(
            root,
            &[
                "commit",
                "--quiet",
                "--author",
                author,
                "-m",
                "change source",
            ],
        );
    }
    fs::create_dir(root.join("workspace1")).expect("create similarly named sibling");
    fs::write(
        root.join("workspace1/outside.ts"),
        "export const outside = true;\n",
    )
    .expect("write outside");
    git(root, &["add", "--", "workspace1/outside.ts"]);
    git(
        root,
        &["commit", "--quiet", "-m", "outside the analyzed project"],
    );

    let config = AnalysisConfig::new(&subproject);
    let report = analyze(&config).expect("analyze Git fixture");
    let history = report.git.expect("Git history");
    assert_eq!(history.commits_analyzed, 4);
    assert_eq!(history.active_files, names.len());

    for file in report.file_graph.files {
        let metrics = file.git.expect("file churn");
        assert_eq!(metrics.commits, 4);
        assert_eq!(metrics.lines_added, 4);
        assert_eq!(metrics.lines_deleted, 3);
        assert_eq!(metrics.top_author, "Alice");
        assert_eq!(metrics.top_author_share, 0.5);
    }

    let pairs: Vec<_> = history
        .temporal_couplings
        .iter()
        .map(|pair| {
            assert_eq!(pair.shared_commits, 4);
            (pair.file_a.as_str(), pair.file_b.as_str())
        })
        .collect();
    let expected = names
        .iter()
        .enumerate()
        .flat_map(|(index, first)| {
            names[index + 1..]
                .iter()
                .map(move |second| (first.as_str(), second.as_str()))
        })
        .take(80)
        .collect::<Vec<_>>();
    assert_eq!(pairs, expected);
}

#[test]
fn temporal_coupling_prioritizes_hidden_cross_module_changes() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    git(root, &["init", "--quiet"]);
    for dir in ["app", "core", "internal"] {
        fs::create_dir(root.join(dir)).expect("create module");
    }

    let files = [
        ("app/api.ts", ""),
        ("app/ui.ts", ""),
        (
            "core/api.ts",
            "import { version as appVersion } from '../app/api';\nexport const upstream = appVersion;\n",
        ),
        ("internal/reconcile.ts", ""),
        ("internal/weak.ts", ""),
    ];
    fs::write(root.join("internal/one_off.ts"), "export const once = 1;\n")
        .expect("write file touched by one commit");
    for version in 0..4 {
        for (name, source) in files {
            fs::write(
                root.join(name),
                format!("{source}export const version = {version};\n"),
            )
            .expect("write source");
        }
        if version == 3 {
            // A bulk edit still counts as churn but contributes no co-change pairs.
            for index in 0..22 {
                fs::write(
                    root.join(format!("bulk_{index:02}.ts")),
                    "export const bulk = true;\n",
                )
                .expect("write bulk edit");
            }
        }
        git(root, &["add", "--", "."]);
        git(root, &["commit", "--quiet", "-m", "change modules"]);
    }
    for version in 4..14 {
        // Three shared commits out of 4 + 14 changes give a strength below 0.35.
        fs::write(
            root.join("internal/weak.ts"),
            format!("export const version = {version};\n"),
        )
        .expect("change weakly coupled file");
        git(
            root,
            &["commit", "--quiet", "--all", "-m", "change one file"],
        );
    }

    let report = analyze(&AnalysisConfig::new(root)).expect("analyze Git fixture");
    let history = report.git.expect("Git history");
    assert_eq!(history.active_files, 28);
    let pairs: Vec<_> = history
        .temporal_couplings
        .iter()
        .map(|pair| {
            assert_eq!(pair.shared_commits, 3);
            assert_eq!((pair.commits_a, pair.commits_b), (4, 4));
            assert_eq!(pair.strength, 0.75);
            (
                pair.file_a.as_str(),
                pair.file_b.as_str(),
                pair.has_static_edge,
                pair.cross_module,
            )
        })
        .collect();
    assert_eq!(
        pairs,
        [
            ("app/api.ts", "internal/reconcile.ts", false, true),
            ("app/ui.ts", "core/api.ts", false, true),
            ("app/ui.ts", "internal/reconcile.ts", false, true),
            ("core/api.ts", "internal/reconcile.ts", false, true),
            ("app/api.ts", "app/ui.ts", false, false),
            ("app/api.ts", "core/api.ts", true, true),
        ],
    );
}

#[test]
fn git_history_uses_literal_paths_and_attributes_renames_to_the_destination() {
    let temp = TempDir::new().expect("tempdir");
    let root_path = temp.path().join(if cfg!(unix) {
        "repo with trailing space "
    } else {
        "repo"
    });
    fs::create_dir(&root_path).expect("create repo");
    let root = root_path.as_path();
    git(root, &["init", "--quiet"]);
    git(root, &["config", "core.quotePath", "true"]);
    git(root, &["config", "diff.renames", "true"]);
    let mut names = vec!["исходник.ts"];
    if cfg!(unix) {
        names.extend(["literal => name.ts", "with\ta tab.ts", "with\na newline.ts"]);
    }

    for name in &names {
        fs::write(root.join(name), "export const value = 1;\n").expect("write source");
    }
    fs::write(root.join("old.ts"), "export const carried = 42;\n").expect("write old path");
    git(root, &["add", "--", "."]);
    git(root, &["commit", "--quiet", "-m", "initial files"]);
    git(root, &["mv", "--", "old.ts", "готово.ts"]);
    git(root, &["commit", "--quiet", "-m", "rename source"]);
    git(
        root,
        &["commit", "--quiet", "--allow-empty", "-m", "empty commit"],
    );

    let report = analyze(&AnalysisConfig::new(root)).expect("analyze Git paths");
    let history = report.git.expect("Git history");
    assert_eq!(history.commits_analyzed, 3);
    assert_eq!(history.active_files, names.len() + 1);

    for file in report.file_graph.files {
        let metrics = file
            .git
            .unwrap_or_else(|| panic!("missing churn for {:?}", file.path));
        assert_eq!(metrics.commits, 1, "{:?}", file.path);
        assert_eq!(
            metrics.lines_added,
            usize::from(file.path != "готово.ts"),
            "{:?}",
            file.path
        );
        assert_eq!(metrics.lines_deleted, 0, "{:?}", file.path);
    }
}

#[test]
fn git_hotspot_weights_incoming_and_outgoing_dependencies_and_recent_history() {
    let temp = TempDir::new().expect("temporary repository");
    let root = temp.path();
    git(root, &["init", "--quiet"]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_secs();
    let commit = |days_ago: u64| {
        let timestamp = now - days_ago * 86_400;
        let date = format!("@{timestamp} +0000");
        git(root, &["add", "--", "."]);
        git_with_env(
            root,
            &["commit", "--quiet", "-m", "update fixture"],
            &[("GIT_COMMITTER_DATE", &date), ("GIT_AUTHOR_DATE", &date)],
        );
    };
    fs::write(root.join("core.ts"), "export const value = 1;\n").expect("core");
    fs::write(root.join("plain.ts"), "export const cache = 1;\n").expect("plain");
    let worker = root.join("worker.ts");
    fs::write(
        &worker,
        "import { value } from './core';\nexport const count = value;\n",
    )
    .expect("worker");
    commit(3);
    fs::write(
        &worker,
        "import { value } from './core';\nexport const count = value + 1;\n",
    )
    .expect("update worker");
    commit(2);
    fs::write(root.join("plain.ts"), "export const cache = 2;\n").expect("update plain");
    commit(1);

    let report = analyze(&AnalysisConfig::new(root)).expect("analyze Git hotspots");
    for (path, score, age) in [
        ("worker.ts", 1.0, 1),
        ("core.ts", 0.3981, 2),
        ("plain.ts", 0.5486, 0),
    ] {
        let metrics = report
            .file_graph
            .files
            .iter()
            .find(|file| file.path == path)
            .and_then(|file| file.git.as_ref())
            .expect("file history");
        assert!(
            (metrics.hotspot_score - score).abs() < 1e-9,
            "{path}: {metrics:?}"
        );
        assert_eq!(metrics.days_since_last_change, age, "{path}");
    }
}

#[test]
fn git_hotspot_is_zero_for_a_repository_with_only_empty_source_files() {
    let temp = TempDir::new().expect("temporary repository");
    let root = temp.path();
    git(root, &["init", "--quiet"]);
    fs::write(root.join("empty.ts"), "").expect("empty source");
    git(root, &["add", "--", "empty.ts"]);
    git(root, &["commit", "--quiet", "-m", "add empty source"]);

    let report = analyze(&AnalysisConfig::new(root)).expect("analyze empty source");
    let file = report.file_graph.files.first().expect("empty source file");
    let metrics = file.git.as_ref().expect("file history");
    assert_eq!(metrics.commits, 1);
    assert!(metrics.hotspot_score.abs() < f64::EPSILON, "{metrics:?}");
}
