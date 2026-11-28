use crate::config::ThresholdConfig;
use crate::domain::metrics::{ArchitectureView, CheckViolation, RuleViolation};

/// Evaluate CI architectural gate thresholds and boundary rule checks against the selected view.
pub fn evaluate_thresholds(
    view: &ArchitectureView,
    thresholds: &ThresholdConfig,
    rule_violations: &[RuleViolation],
) -> Vec<CheckViolation> {
    let mut violations = Vec::new();

    if thresholds.fail_on_cycles && !view.cycles.is_empty() {
        let preview: Vec<String> = view
            .cycles
            .iter()
            .take(3)
            .map(|c| c.nodes.join(" <-> "))
            .collect();
        violations.push(CheckViolation {
            rule: "fail_on_cycles".to_string(),
            message: format!(
                "found {} dependency cycle(s) in '{}': {}",
                view.cycles.len(),
                view.id,
                preview.join("; ")
            ),
        });
    }

    if thresholds.fail_on_rule_violations && !rule_violations.is_empty() {
        for rv in rule_violations.iter().take(10) {
            violations.push(CheckViolation {
                rule: format!("arch_rule:{}", rv.rule_name),
                message: format!(
                    "{}:{} -> {} ({})",
                    rv.from_file, rv.line, rv.to_file, rv.message
                ),
            });
        }
        if rule_violations.len() > 10 {
            violations.push(CheckViolation {
                rule: "arch_rule".to_string(),
                message: format!(
                    "...and {} more architecture rule violation(s)",
                    rule_violations.len() - 10
                ),
            });
        }
    }

    if let Some(max_dist) = thresholds.max_distance {
        for node in &view.nodes {
            if let Some(d) = node.distance
                && d > max_dist
            {
                violations.push(CheckViolation {
                    rule: "max_distance".to_string(),
                    message: format!(
                        "component '{}' has distance D={:.2} > {:.2} (I={:.2}, A={:.2})",
                        node.id,
                        d,
                        max_dist,
                        node.instability.unwrap_or(0.0),
                        node.abstractness.unwrap_or(0.0),
                    ),
                });
            }
        }
    }

    if let Some(max_sdp) = thresholds.max_sdp_violations
        && view.sdp_violations.len() > max_sdp
    {
        violations.push(CheckViolation {
            rule: "max_sdp_violations".to_string(),
            message: format!(
                "found {} SDP violation(s) in '{}' (limit: {max_sdp})",
                view.sdp_violations.len(),
                view.id
            ),
        });
    }

    if let Some(max_pain) = thresholds.max_zone_of_pain
        && view.summary.zone_of_pain_count > max_pain
    {
        violations.push(CheckViolation {
            rule: "max_zone_of_pain".to_string(),
            message: format!(
                "found {} component(s) in the Zone of Pain in '{}' (limit: {max_pain})",
                view.summary.zone_of_pain_count, view.id
            ),
        });
    }

    if let Some(max_pc) = thresholds.max_propagation_cost
        && view.summary.propagation_cost > max_pc
    {
        violations.push(CheckViolation {
            rule: "max_propagation_cost".to_string(),
            message: format!(
                "propagation cost in '{}' is {:.1}% > {:.1}%",
                view.id,
                view.summary.propagation_cost * 100.0,
                max_pc * 100.0
            ),
        });
    }

    violations
}
