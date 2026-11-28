use std::fs;
use std::path::Path;

use archlens::diff::load_report_json;
use archlens::report::html::write_html_report;
use archlens::report::json::write_json_report;
use archlens::{AnalysisConfig, AnalysisReport, analyze};
use tempfile::TempDir;

fn empty_report(root: &Path) -> AnalysisReport {
    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    analyze(&config).expect("analyze empty directory")
}

#[test]
fn report_exports_preserve_data_and_create_parent_directories() {
    let temp = TempDir::new().expect("tempdir");
    let mut report = empty_report(temp.path());
    report.project_name = "project __ARCHLENS_MODEL_JS__".into();

    let json_path = temp.path().join("json/nested/report.json");
    write_json_report(&report, &json_path).expect("write JSON");
    let roundtrip = load_report_json(&json_path).expect("load JSON through the public diff path");
    assert_eq!(roundtrip.project_name, report.project_name);

    let html_path = temp.path().join("html/nested/report.html");
    write_html_report(&report, &html_path).expect("write HTML");
    let html = fs::read_to_string(html_path).expect("read HTML");
    let data_start = r#"<script id="archlens-data" type="application/json">"#;
    let (_, after_start) = html.split_once(data_start).expect("HTML report data tag");
    let (payload, _) = after_start.split_once("</script>").expect("data tag end");
    let embedded: AnalysisReport = serde_json::from_str(payload).expect("valid embedded JSON");
    assert_eq!(embedded.project_name, roundtrip.project_name);
}
