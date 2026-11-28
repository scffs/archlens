use std::collections::BTreeSet;

use crate::domain::metrics::{AnalysisReport, BaselineDiff, RuleViolation, round4};
pub use crate::report::json::load_report_json;

/// Compare `current` against `baseline` for the specified view (or `current.default_view`).
pub fn compute_baseline_diff(
    baseline: &AnalysisReport,
    current: &AnalysisReport,
    view_id: Option<&str>,
) -> Option<BaselineDiff> {
    let cur_view = current.view_or_first(view_id)?;
    let base_view = baseline.view_or_first(Some(&cur_view.id))?;

    let base_cycles: BTreeSet<Vec<String>> = base_view
        .cycles
        .iter()
        .map(|c| {
            let mut n = c.nodes.clone();
            n.sort();
            n
        })
        .collect();
    let cur_cycles: BTreeSet<Vec<String>> = cur_view
        .cycles
        .iter()
        .map(|c| {
            let mut n = c.nodes.clone();
            n.sort();
            n
        })
        .collect();

    let new_cycles: Vec<Vec<String>> = cur_cycles.difference(&base_cycles).cloned().collect();
    let resolved_cycles: Vec<Vec<String>> = base_cycles.difference(&cur_cycles).cloned().collect();

    let base_edges: BTreeSet<(String, String)> = base_view
        .edges
        .iter()
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();
    let cur_edges: BTreeSet<(String, String)> = cur_view
        .edges
        .iter()
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();

    let new_edges: Vec<(String, String)> = cur_edges
        .difference(&base_edges)
        .take(50)
        .cloned()
        .collect();
    let removed_edges: Vec<(String, String)> = base_edges
        .difference(&cur_edges)
        .take(50)
        .cloned()
        .collect();

    let base_rule_keys: BTreeSet<(&str, &str, &str)> = baseline
        .rule_violations
        .iter()
        .map(|r| (r.rule_name.as_str(), r.from_file.as_str(), r.to_file.as_str()))
        .collect();
    let new_rule_violations: Vec<RuleViolation> = current
        .rule_violations
        .iter()
        .filter(|r| {
            !base_rule_keys.contains(&(
                r.rule_name.as_str(),
                r.from_file.as_str(),
                r.to_file.as_str(),
            ))
        })
        .cloned()
        .collect();

    let delta_average_distance = match (
        base_view.summary.average_distance,
        cur_view.summary.average_distance,
    ) {
        (Some(b), Some(c)) => Some(round4(c - b)),
        _ => None,
    };

    Some(BaselineDiff {
        baseline_view: cur_view.id.clone(),
        delta_nodes: diff_usize(cur_view.summary.node_count, base_view.summary.node_count),
        delta_edges: diff_usize(cur_view.summary.edge_count, base_view.summary.edge_count),
        delta_cycles: diff_usize(cur_view.summary.cycle_count, base_view.summary.cycle_count),
        delta_sdp_violations: diff_usize(
            cur_view.summary.sdp_violation_count,
            base_view.summary.sdp_violation_count,
        ),
        delta_rule_violations: diff_usize(
            current.rule_violations.len(),
            baseline.rule_violations.len(),
        ),
        delta_propagation_cost: round4(
            cur_view.summary.propagation_cost - base_view.summary.propagation_cost,
        ),
        delta_average_distance,
        new_cycles,
        resolved_cycles,
        new_edges,
        removed_edges,
        new_rule_violations,
    })
}

fn diff_usize(cur: usize, base: usize) -> isize {
    let c = isize::try_from(cur).unwrap_or(isize::MAX);
    let b = isize::try_from(base).unwrap_or(isize::MAX);
    c - b
}
