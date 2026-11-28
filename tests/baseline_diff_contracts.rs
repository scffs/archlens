use std::fs;

use archlens::diff::compute_baseline_diff;
use archlens::domain::metrics::RuleViolation;
use archlens::report::terminal::render_diff_summary;
use archlens::{AnalysisConfig, analyze};
use tempfile::TempDir;

#[test]
fn baseline_diff_reports_introduced_and_resolved_graph_changes() {
    let project = TempDir::new().expect("temporary project");
    let root = project.path();
    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let rule = |name: &str, from: &str, to: &str| RuleViolation {
        rule_name: name.into(),
        kind: "forbidden".into(),
        from_file: from.into(),
        to_file: to.into(),
        from_module: "<root>".into(),
        to_module: "<root>".into(),
        line: 1,
        symbols: Vec::new(),
        message: "imports are not allowed here".into(),
    };

    fs::write(root.join("a.ts"), "import './b';\nexport function a() {}\n").expect("write a");
    fs::write(root.join("b.ts"), "import './a';\nexport function b() {}\n").expect("write b");
    let mut before = analyze(&config).expect("analyze baseline");
    let persistent = rule("existing", "a.ts", "b.ts");
    before.rule_violations.push(persistent.clone());

    fs::write(root.join("b.ts"), "import './c';\nexport function b() {}\n").expect("redirect b");
    fs::write(root.join("c.ts"), "import './b';\nexport function c() {}\n").expect("write c");
    let mut current = analyze(&config).expect("analyze updated project");
    current.rule_violations = vec![persistent, rule("new", "b.ts", "c.ts")];

    let diff = compute_baseline_diff(&before, &current, Some("files")).expect("file diff");
    assert_eq!(
        (diff.delta_nodes, diff.delta_edges, diff.delta_cycles),
        (1, 1, 0)
    );
    assert_eq!(diff.new_cycles, [["b.ts", "c.ts"]]);
    assert_eq!(diff.resolved_cycles, [["a.ts", "b.ts"]]);
    assert!((diff.delta_propagation_cost - -0.2222).abs() < 1e-9);
    assert_eq!(diff.delta_average_distance, Some(-0.1111));
    assert_eq!(
        diff.new_rule_violations
            .iter()
            .map(|r| r.rule_name.as_str())
            .collect::<Vec<_>>(),
        ["new"]
    );

    let text = render_diff_summary(&diff);
    for expected in [
        "Baseline Diff (view: files):",
        "Nodes: +1 | Edges: +1 | Cycles: +0",
        "PropCost: -22.2%",
        "NEW cycles (1):\n    + b.ts <-> c.ts",
        "Resolved cycles (1):\n    - a.ts <-> b.ts",
        "NEW rule violations (1):\n    + [new] b.ts -> c.ts",
    ] {
        assert!(text.contains(expected), "{expected:?} is missing:\n{text}");
    }
    assert!(!text.contains("[existing]"), "{text}");
}
