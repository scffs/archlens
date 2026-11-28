use std::fmt::Write as _;

use crate::domain::metrics::{AnalysisReport, ArchitectureView, ArchitectureZone, BaselineDiff};

/// Render a concise, readable Clean Architecture summary for terminal output.
pub fn render_terminal_summary(report: &AnalysisReport, view_id: Option<&str>) -> String {
    let mut out = String::new();

    let view = report.view_or_first(view_id);

    let langs: Vec<String> = report
        .language_counts
        .iter()
        .map(|(lang, count)| format!("{lang}: {count}"))
        .collect();

    let _ = writeln!(out, "archlens — {}", report.project_name);
    let _ = writeln!(
        out,
        "Files: {} | LOC: {} | Internal edges: {} | Unresolved local imports: {}",
        report.total_files,
        report.total_loc,
        report.total_internal_edges,
        report.unresolved_import_count
    );
    if !langs.is_empty() {
        let _ = writeln!(out, "Languages: {}", langs.join(", "));
    }
    if let Some(ref git) = report.git {
        let hidden_temporal = git
            .temporal_couplings
            .iter()
            .filter(|t| !t.has_static_edge && t.cross_module)
            .count();
        let _ = writeln!(
            out,
            "Git ({}d): {} commits | {} active files | {} temporal couplings ({} hidden cross-module)",
            git.lookback_days,
            git.commits_analyzed,
            git.active_files,
            git.temporal_couplings.len(),
            hidden_temporal
        );
    }

    let Some(view) = view else {
        let _ = writeln!(out, "\nNo source files found.");
        return out;
    };

    render_view_section(&mut out, view);

    if !report.rule_violations.is_empty() {
        let _ = writeln!(
            out,
            "\nArchitecture rule violations ({}, showing up to 8):",
            report.rule_violations.len()
        );
        for rv in report.rule_violations.iter().take(8) {
            let _ = writeln!(
                out,
                "  - [{}] {}:{} -> {} ({})",
                rv.rule_name, rv.from_file, rv.line, rv.to_file, rv.message
            );
        }
    }

    if !report.misplaced_files.is_empty() {
        let _ = writeln!(
            out,
            "\nPotential misplaced files by Louvain affinity (top 5):"
        );
        for mf in report.misplaced_files.iter().take(5) {
            let _ = writeln!(
                out,
                "  - {} (in {}, affinity {:.0}% -> {}; {} vs {} links)",
                mf.file,
                mf.current_module,
                mf.affinity * 100.0,
                mf.suggested_module,
                mf.suggested_module_edges,
                mf.internal_edges
            );
        }
    }

    if let Some(ref diff) = report.baseline_diff {
        let _ = writeln!(out);
        out.push_str(&render_diff_summary(diff));
    }

    if !report.check_violations.is_empty() {
        let _ = writeln!(
            out,
            "\nThreshold / rule check failures ({}):",
            report.check_violations.len()
        );
        for v in &report.check_violations {
            let _ = writeln!(out, "  [FAIL] {}: {}", v.rule, v.message);
        }
    }

    out
}

/// Render a human-readable baseline diff summary.
pub fn render_diff_summary(diff: &BaselineDiff) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Baseline Diff (view: {}):", diff.baseline_view);
    let _ = writeln!(
        out,
        "  Nodes: {:+} | Edges: {:+} | Cycles: {:+} | SDP: {:+} | Rules: {:+} | PropCost: {:+.1}%",
        diff.delta_nodes,
        diff.delta_edges,
        diff.delta_cycles,
        diff.delta_sdp_violations,
        diff.delta_rule_violations,
        diff.delta_propagation_cost * 100.0
    );
    if !diff.new_cycles.is_empty() {
        let _ = writeln!(out, "  NEW cycles ({}):", diff.new_cycles.len());
        for c in &diff.new_cycles {
            let _ = writeln!(out, "    + {}", c.join(" <-> "));
        }
    }
    if !diff.resolved_cycles.is_empty() {
        let _ = writeln!(out, "  Resolved cycles ({}):", diff.resolved_cycles.len());
        for c in &diff.resolved_cycles {
            let _ = writeln!(out, "    - {}", c.join(" <-> "));
        }
    }
    if !diff.new_rule_violations.is_empty() {
        let _ = writeln!(
            out,
            "  NEW rule violations ({}):",
            diff.new_rule_violations.len()
        );
        for r in diff.new_rule_violations.iter().take(8) {
            let _ = writeln!(out, "    + [{}] {} -> {}", r.rule_name, r.from_file, r.to_file);
        }
    }
    out
}

fn render_view_section(out: &mut String, view: &ArchitectureView) {
    let s = &view.summary;
    let _ = writeln!(out, "\nView: {} ({})", view.title, view.id);
    let _ = writeln!(
        out,
        "Nodes: {} | Edges: {} | Cycles: {} ({} nodes) | SDP: {} | PropCost: {:.1}% | ACD: {:.2} | NCCD: {:.2}",
        s.node_count,
        s.edge_count,
        s.cycle_count,
        s.nodes_in_cycles,
        s.sdp_violation_count,
        s.propagation_cost * 100.0,
        s.acd,
        s.nccd
    );
    let _ = writeln!(
        out,
        "Zones: MainSeq={} | Pain={} (active={}) | Useless={} | Isolated={} | NoEntities={}",
        s.main_sequence_count,
        s.zone_of_pain_count,
        s.active_pain_count,
        s.zone_of_uselessness_count,
        s.isolated_count,
        s.no_entities_count
    );
    if let (Some(avg_d), Some(max_d)) = (s.average_distance, s.max_distance) {
        let _ = writeln!(
            out,
            "Main Sequence Distance D: avg={avg_d:.2}, max={max_d:.2}"
        );
    }

    let mut ranked: Vec<_> = view.nodes.iter().collect();
    ranked.sort_by(|a, b| {
        b.active_pain
            .cmp(&a.active_pain)
            .then_with(|| b.distance.unwrap_or(-1.0).total_cmp(&a.distance.unwrap_or(-1.0)))
            .then_with(|| {
                (b.afferent_coupling + b.efferent_coupling)
                    .cmp(&(a.afferent_coupling + a.efferent_coupling))
            })
            .then_with(|| a.id.cmp(&b.id))
    });

    if !ranked.is_empty() {
        let _ = writeln!(
            out,
            "\nTop components (active pain & Distance D, up to 18):"
        );
        let _ = writeln!(
            out,
            "  {:<32} {:>4} {:>4} {:>5} {:>5} {:>5} {:>6} {:>5}  {:<11} Cyc",
            "Component", "Ca", "Ce", "I", "A", "D", "Cmts", "Hot", "Zone"
        );
        let _ = writeln!(out, "  {}", "-".repeat(96));
        for node in ranked.into_iter().take(18) {
            let name = truncate(&node.id, 32);
            let i_str = fmt_opt(node.instability);
            let a_str = fmt_opt(node.abstractness);
            let d_str = fmt_opt(node.distance);
            let zone = if node.active_pain {
                "ACTIVE-PAIN"
            } else {
                zone_short(node.zone)
            };
            let cyc = if node.in_cycle { "YES" } else { "-" };
            let _ = writeln!(
                out,
                "  {:<32} {:>4} {:>4} {:>5} {:>5} {:>5} {:>6} {:>5.2}  {:<11} {}",
                name,
                node.afferent_coupling,
                node.efferent_coupling,
                i_str,
                a_str,
                d_str,
                node.commits,
                node.hotspot_score,
                zone,
                cyc
            );
        }
    }

    if !view.cycles.is_empty() {
        let _ = writeln!(out, "\nDependency cycles (showing up to 8):");
        for cycle in view.cycles.iter().take(8) {
            let _ = writeln!(
                out,
                "  - [{} nodes, {} edges] {}",
                cycle.nodes.len(),
                cycle.edge_count,
                cycle.nodes.join(" <-> ")
            );
            for brk in &cycle.break_suggestions {
                let _ = writeln!(
                    out,
                    "      cut suggestion: {} -> {} ({} file edge(s))",
                    brk.from, brk.to, brk.weight
                );
            }
        }
    }

    if !view.sdp_violations.is_empty() {
        let _ = writeln!(
            out,
            "\nTop SDP violations (stable -> less stable, up to 8):"
        );
        for v in view.sdp_violations.iter().take(8) {
            let dip_tag = if v.is_dip_only { " [DIP-only]" } else { "" };
            let _ = writeln!(
                out,
                "  - {} (I={:.2}) -> {} (I={:.2})  [delta=+{:.2}, {} edge(s)]{}",
                v.from, v.from_instability, v.to, v.to_instability, v.delta, v.weight, dip_tag
            );
        }
    }
}

fn fmt_opt(val: Option<f64>) -> String {
    match val {
        Some(v) => format!("{v:.2}"),
        None => "-".to_string(),
    }
}

fn zone_short(zone: ArchitectureZone) -> &'static str {
    match zone {
        ArchitectureZone::MainSequence => "main-seq",
        ArchitectureZone::ZoneOfPain => "pain(calm)",
        ArchitectureZone::ZoneOfUselessness => "USELESS",
        ArchitectureZone::Isolated => "isolated",
        ArchitectureZone::NoEntities => "no-entities",
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        return s.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let tail: String = chars[chars.len() - keep..].iter().collect();
    format!("…{tail}")
}
