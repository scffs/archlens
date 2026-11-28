use std::fs;
use std::path::Path;
use std::process::Command;

use archlens::{AnalysisConfig, analyze};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join(".git/nonexistent-config"))
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
