use std::collections::HashMap;

use globset::GlobSet;

use crate::config::{ArchitectureRule, LayerRule, RuleKind};
use crate::domain::metrics::RuleViolation;
use crate::domain::model::FileGraph;
use crate::error::Result;
use crate::paths::build_glob_set;

struct CompiledRule {
    name: String,
    kind: RuleKind,
    from: GlobSet,
    to: GlobSet,
    allow_dip: bool,
    reason: Option<String>,
}

struct CompiledLayer {
    name: String,
    matcher: GlobSet,
    allow_dip: bool,
}

/// Evaluate all declarative boundary and layering rules against the resolved file graph.
pub fn evaluate_architecture_rules(
    graph: &FileGraph,
    layers: &[LayerRule],
    rules: &[ArchitectureRule],
) -> Result<Vec<RuleViolation>> {
    if layers.is_empty() && rules.is_empty() {
        return Ok(Vec::new());
    }

    let module_by_file: HashMap<&str, &str> = graph
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.module_id.as_str()))
        .collect();

    let mut compiled_layers = Vec::with_capacity(layers.len());
    for layer in layers {
        compiled_layers.push(CompiledLayer {
            name: layer.name.clone(),
            matcher: build_glob_set(&layer.patterns)?,
            allow_dip: layer.allow_dip,
        });
    }

    let mut compiled_rules = Vec::with_capacity(rules.len());
    for rule in rules {
        compiled_rules.push(CompiledRule {
            name: rule.name.clone(),
            kind: rule.kind,
            from: build_glob_set(&rule.from)?,
            to: build_glob_set(&rule.to)?,
            allow_dip: rule.allow_dip,
            reason: rule.reason.clone(),
        });
    }

    let mut violations = Vec::new();

    for edge in &graph.edges {
        let from_module = module_by_file
            .get(edge.from.as_str())
            .copied()
            .unwrap_or("")
            .to_string();
        let to_module = module_by_file
            .get(edge.to.as_str())
            .copied()
            .unwrap_or("")
            .to_string();

        // 1. Evaluate ordered layers (index 0 = top-most; lower index may depend on higher index, never reverse).
        if !compiled_layers.is_empty() {
            let from_layer = compiled_layers
                .iter()
                .position(|l| l.matcher.is_match(&edge.from));
            let to_layer = compiled_layers
                .iter()
                .position(|l| l.matcher.is_match(&edge.to));

            if let (Some(fi), Some(ti)) = (from_layer, to_layer)
                && fi > ti
                && !(compiled_layers[fi].allow_dip && edge.is_dip)
            {
                let fl = &compiled_layers[fi].name;
                let tl = &compiled_layers[ti].name;
                violations.push(RuleViolation {
                    rule_name: format!("layer:{fl}->{tl}"),
                    kind: "layer".to_string(),
                    from_file: edge.from.clone(),
                    to_file: edge.to.clone(),
                    from_module: from_module.clone(),
                    to_module: to_module.clone(),
                    line: edge.line,
                    symbols: edge.symbols.clone(),
                    message: format!("lower layer '{fl}' must not depend on upper layer '{tl}'"),
                });
            }
        }

        // 2. Evaluate explicit boundary / facade rules.
        for rule in &compiled_rules {
            if rule.allow_dip && edge.is_dip {
                continue;
            }
            let violated = match rule.kind {
                RuleKind::Forbid => rule.from.is_match(&edge.from) && rule.to.is_match(&edge.to),
                RuleKind::AllowOnly => {
                    rule.from.is_match(&edge.from)
                        && !rule.from.is_match(&edge.to)
                        && !rule.to.is_match(&edge.to)
                }
                RuleKind::PublicApi => {
                    !rule.from.is_match(&edge.from)
                        && rule.from.is_match(&edge.to)
                        && !rule.to.is_match(&edge.to)
                }
            };

            if violated {
                let kind_str = match rule.kind {
                    RuleKind::Forbid => "forbid",
                    RuleKind::AllowOnly => "allow_only",
                    RuleKind::PublicApi => "public_api",
                };
                let default_msg = match rule.kind {
                    RuleKind::Forbid => {
                        format!("forbidden dependency matched rule '{}'", rule.name)
                    }
                    RuleKind::AllowOnly => format!(
                        "dependency outside allowed targets for rule '{}'",
                        rule.name
                    ),
                    RuleKind::PublicApi => {
                        format!("bypasses public facade required by rule '{}'", rule.name)
                    }
                };
                violations.push(RuleViolation {
                    rule_name: rule.name.clone(),
                    kind: kind_str.to_string(),
                    from_file: edge.from.clone(),
                    to_file: edge.to.clone(),
                    from_module: from_module.clone(),
                    to_module: to_module.clone(),
                    line: edge.line,
                    symbols: edge.symbols.clone(),
                    message: rule.reason.clone().unwrap_or(default_msg),
                });
            }
        }
    }

    violations.sort_by(|a, b| {
        a.rule_name
            .cmp(&b.rule_name)
            .then_with(|| a.from_file.cmp(&b.from_file))
            .then_with(|| a.to_file.cmp(&b.to_file))
    });
    Ok(violations)
}
