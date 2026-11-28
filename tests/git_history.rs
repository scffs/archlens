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

    let subproject = root.join("workspace");
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
        git(root, &["add", "--", "workspace"]);
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
    fs::write(root.join("outside.ts"), "export const outside = true;\n").expect("write outside");
    git(root, &["add", "--", "outside.ts"]);
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
