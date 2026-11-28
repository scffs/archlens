use std::fs;

use archlens::report::terminal::render_terminal_summary;
use archlens::{AnalysisConfig, analyze};
use tempfile::TempDir;

#[test]
fn terminal_summary_shows_ranked_components_cycles_and_unicode_paths() {
    let temp = TempDir::new().expect("temporary project");
    let root = temp.path();
    let long_path = format!("{}tail.ts", "界".repeat(40));
    for (path, source) in [
        ("a-light.ts", "export function light() { return 1; }\n"),
        ("z-heavy.ts", "export function heavy() { return 1; }\n"),
        (
            "entry-first.ts",
            "import { light } from './a-light';\nimport { heavy } from './z-heavy';\nexport function out() { return light + heavy; }\n",
        ),
        (
            "entry-second.ts",
            "import { heavy } from './z-heavy';\nexport function out() { return heavy; }\n",
        ),
        (
            "cycle-a.ts",
            "import './cycle-b';\nexport function a() { return 1; }\n",
        ),
        (
            "cycle-b.ts",
            "import './cycle-a';\nexport function b() { return 1; }\n",
        ),
        (
            long_path.as_str(),
            "export function isolated() { return true; }\n",
        ),
    ] {
        fs::write(root.join(path), source).expect("fixture source");
    }
    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let report = analyze(&config).expect("analyze terminal fixture");
    let text = render_terminal_summary(&report, Some("files"));

    assert!(text.contains("Languages: TypeScript: 7"), "{text}");
    assert!(text.contains("Dependency cycles"), "{text}");
    assert!(text.contains("cycle-a.ts <-> cycle-b.ts"), "{text}");
    assert!(text.contains("cut suggestion:"), "{text}");
    for absent in [
        "Top SDP violations",
        "Architecture rule violations",
        "Potential misplaced files",
        "Threshold / rule check failures",
    ] {
        assert!(!text.contains(absent), "{text}");
    }
    let name = format!("…{}tail.ts", "界".repeat(24));
    let rows = |id: &str| {
        text.lines()
            .enumerate()
            .find(|(_, row)| row.split_whitespace().next() == Some(id))
            .unwrap_or_else(|| panic!("missing component {id} in {text}"))
    };
    let (heavy, _) = rows("z-heavy.ts");
    let (light, row) = rows("a-light.ts");
    let (isolated, isolated_row) = rows(&name);
    let columns: Vec<_> = row.split_whitespace().collect();
    assert_eq!(
        &columns[..6],
        ["a-light.ts", "1", "0", "0.00", "0.00", "1.00"]
    );
    assert!(row.contains("pain(calm)"), "{row}");
    assert!(heavy < light && light < isolated, "{text}");
    assert!(isolated_row.contains("isolated"), "{isolated_row}");
    assert_eq!(isolated_row.split_whitespace().nth(5), Some("-"));
}
