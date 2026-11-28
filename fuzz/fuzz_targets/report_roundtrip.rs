#![no_main]

mod support;

use std::fs;

use archlens::{
    AnalysisConfig, AnalysisReport, analyze_file_graph,
    diff::{compute_baseline_diff, load_report_json},
    report::{html::render_html_report, json::write_json_report},
};
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

fuzz_target!(|input: &[u8]| {
    let Some((&selector, payload)) = input.split_first() else {
        return;
    };
    let title_len = usize::from(selector % 33).min(payload.len());
    let (title, graph_bytes) = payload.split_at(title_len);
    let title: String = String::from_utf8_lossy(title)
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    let graph = support::file_graph(graph_bytes);

    let temp = tempfile::tempdir().expect("create a temporary report directory");
    let root = temp.path();
    let mut report = analyze_file_graph(root, &graph, &AnalysisConfig::new(root))
        .expect("analyze a valid synthetic graph");
    report.project_name = format!("Archlens {title}");

    let path = root.join("report.json");
    write_json_report(&report, &path).expect("write a JSON snapshot");
    let loaded = load_report_json(&path).expect("read the JSON snapshot");
    assert_snapshot(&report, &loaded);

    let html = render_html_report(&report).expect("render the HTML report");
    let (_, data) = html
        .split_once(r#"<script id="archlens-data" type="application/json">"#)
        .expect("report data tag");
    let (payload, _) = data
        .split_once("</script>")
        .expect("end of report data tag");
    let embedded: Value = serde_json::from_str(payload).expect("read the HTML data");
    let plain: Value = serde_json::from_slice(&fs::read(path).expect("read the JSON file"))
        .expect("read the JSON data");
    assert_eq!(plain, embedded);
});

fn assert_snapshot(report: &AnalysisReport, loaded: &AnalysisReport) {
    assert_eq!(loaded.project_name, report.project_name);
    assert_eq!(loaded.root_path, report.root_path);
    assert_eq!(loaded.default_view, report.default_view);
    assert_eq!(loaded.total_files, report.total_files);
    assert_eq!(loaded.total_loc, report.total_loc);
    assert_eq!(loaded.total_internal_edges, report.total_internal_edges);
    assert_eq!(loaded.language_counts, report.language_counts);
    assert_eq!(loaded.file_graph.edges, report.file_graph.edges);
    assert_eq!(loaded.file_graph.files.len(), report.file_graph.files.len());
    for (original, restored) in report.file_graph.files.iter().zip(&loaded.file_graph.files) {
        assert_eq!(restored.path, original.path);
        assert_eq!(restored.module_id, original.module_id);
        assert_eq!(restored.language, original.language);
        assert_eq!(restored.loc, original.loc);
    }
    assert_eq!(loaded.views.len(), report.views.len());
    for (original, restored) in report.views.iter().zip(&loaded.views) {
        assert_eq!(original.id, restored.id);
        assert_eq!(original.dsm_order, restored.dsm_order);
        let diff = compute_baseline_diff(report, loaded, Some(&original.id))
            .expect("diff the restored view against the original");
        assert_eq!(diff.delta_nodes, 0);
        assert_eq!(diff.delta_edges, 0);
        assert_eq!(diff.delta_cycles, 0);
        assert_eq!(diff.delta_sdp_violations, 0);
        assert_eq!(diff.delta_rule_violations, 0);
        assert_eq!(diff.delta_propagation_cost, 0.0);
        assert_eq!(
            diff.delta_average_distance,
            original.summary.average_distance.map(|_| 0.0),
        );
        assert!(diff.new_edges.is_empty() && diff.removed_edges.is_empty());
        assert!(diff.new_cycles.is_empty() && diff.resolved_cycles.is_empty());
        assert!(diff.new_rule_violations.is_empty());
    }
}
