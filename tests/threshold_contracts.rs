use std::fs;

use archlens::checks::evaluate_thresholds;
use archlens::{AnalysisConfig, ArchitectureView, RuleViolation, ThresholdConfig, analyze};
use tempfile::TempDir;

fn reference_view() -> ArchitectureView {
    let temp = TempDir::new().expect("tempdir");
    for (file, source) in [
        (
            "caller.ts",
            "import './gateway'; export interface Port {} export class Caller {}",
        ),
        (
            "gateway.ts",
            "import './worker'; import './core'; export interface Port {} export class Gateway {} export class Store {}",
        ),
        (
            "worker.ts",
            "import './caller'; export interface Port {} export class Worker {}",
        ),
        (
            "core.ts",
            "export interface Port {} export class Core {} export class Model {} export class Engine {}",
        ),
    ] {
        fs::write(temp.path().join(file), source).expect("write TypeScript fixture");
    }
    let mut config = AnalysisConfig::new(temp.path());
    config.git.enabled = false;
    analyze(&config)
        .expect("analyze fixture")
        .views
        .into_iter()
        .find(|view| view.id == "files")
        .expect("files view")
}

fn assert_threshold<Limit>(
    view: &ArchitectureView,
    [below, equal, above]: [Limit; 3],
    thresholds: impl Fn(Limit) -> ThresholdConfig,
    expected: (&str, &str),
) {
    let violations = evaluate_thresholds(view, &thresholds(below), &[]);
    let reported: Vec<_> = violations
        .iter()
        .map(|violation| (violation.rule.as_str(), violation.message.as_str()))
        .collect();
    assert_eq!(reported, [expected]);
    assert!(
        evaluate_thresholds(view, &thresholds(equal), &[]).is_empty(),
        "{} should allow equality",
        expected.0
    );
    assert!(
        evaluate_thresholds(view, &thresholds(above), &[]).is_empty(),
        "{} should allow a higher limit",
        expected.0
    );
}

#[test]
fn numeric_thresholds_fail_only_when_the_report_exceeds_the_limit() {
    let view = reference_view();
    assert!(evaluate_thresholds(&view, &ThresholdConfig::default(), &[]).is_empty());

    assert_threshold(
        &view,
        [0.5, 0.75, 1.0],
        |limit| ThresholdConfig {
            max_distance: Some(limit),
            ..ThresholdConfig::default()
        },
        (
            "max_distance",
            "component 'core.ts' has distance D=0.75 > 0.50 (I=0.00, A=0.25)",
        ),
    );
    assert_threshold(
        &view,
        [0, 1, 2],
        |limit| ThresholdConfig {
            max_sdp_violations: Some(limit),
            ..ThresholdConfig::default()
        },
        (
            "max_sdp_violations",
            "found 1 SDP violation(s) in 'files' (limit: 0)",
        ),
    );
    assert_threshold(
        &view,
        [0, 1, 2],
        |limit| ThresholdConfig {
            max_zone_of_pain: Some(limit),
            ..ThresholdConfig::default()
        },
        (
            "max_zone_of_pain",
            "found 1 component(s) in the Zone of Pain in 'files' (limit: 0)",
        ),
    );
    // The three members of the cycle can each reach four files, and core reaches only itself.
    assert_threshold(
        &view,
        [0.5, 0.8125, 1.0],
        |limit| ThresholdConfig {
            max_propagation_cost: Some(limit),
            ..ThresholdConfig::default()
        },
        (
            "max_propagation_cost",
            "propagation cost in 'files' is 81.2% > 50.0%",
        ),
    );
}

#[test]
fn rule_threshold_reports_the_first_ten_and_counts_only_the_remainder() {
    let view = reference_view();
    let violations: Vec<_> = (1..=12)
        .map(|line| RuleViolation {
            rule_name: "no-core".into(),
            kind: "forbid".into(),
            from_file: "caller.ts".into(),
            to_file: "core.ts".into(),
            from_module: "caller".into(),
            to_module: "core".into(),
            line,
            symbols: Vec::new(),
            message: "forbidden dependency".into(),
        })
        .collect();

    assert!(evaluate_thresholds(&view, &ThresholdConfig::default(), &violations).is_empty());
    let thresholds = ThresholdConfig {
        fail_on_rule_violations: true,
        ..ThresholdConfig::default()
    };
    assert_eq!(
        evaluate_thresholds(&view, &thresholds, &violations[..10]).len(),
        10
    );

    let reported = evaluate_thresholds(&view, &thresholds, &violations);
    assert_eq!(reported.len(), 11);
    assert_eq!(reported[0].rule, "arch_rule:no-core");
    assert_eq!(
        reported[0].message,
        "caller.ts:1 -> core.ts (forbidden dependency)"
    );
    assert_eq!(
        reported[9].message,
        "caller.ts:10 -> core.ts (forbidden dependency)"
    );
    assert_eq!(reported[10].rule, "arch_rule");
    assert_eq!(
        reported[10].message,
        "...and 2 more architecture rule violation(s)"
    );
}
