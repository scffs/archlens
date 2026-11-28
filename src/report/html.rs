use std::path::Path;

use crate::domain::metrics::AnalysisReport;
use crate::error::Result;
use crate::report::output::write_report;

const HTML_SHELL: &str = include_str!("shell.html");
const STYLE_CSS: &str = include_str!("assets/style.css");
const SCRIPT_JS: &str = concat!(
    include_str!("assets/model.js"),
    "\n",
    include_str!("assets/layout.js"),
    "\n",
    include_str!("assets/graph.js"),
    "\n",
    include_str!("assets/dsm.js"),
    "\n",
    include_str!("assets/charts.js"),
    "\n",
    include_str!("assets/inspector.js"),
    "\n",
    include_str!("assets/app.js"),
);

/// Render a fully self-contained interactive HTML report (no CDN or server required).
pub fn render_html_report(report: &AnalysisReport) -> Result<String> {
    let json = serde_json::to_string(report)?;
    // Prevent `</script>` sequences inside serialized file paths from prematurely closing the script tag.
    let safe_json = json.replace("</", "<\\/");

    let slots = [
        ("__ARCHLENS_STYLE_CSS__", STYLE_CSS),
        ("__ARCHLENS_REPORT_JSON__", &safe_json),
        ("__ARCHLENS_SCRIPT_JS__", SCRIPT_JS),
    ];
    let mut html = String::with_capacity(
        HTML_SHELL.len() + slots.iter().map(|(_, value)| value.len()).sum::<usize>(),
    );
    let mut rest = HTML_SHELL;
    for (slot, value) in slots {
        let (before, after) = rest.split_once(slot).expect("ordered HTML template slots");
        html.push_str(before);
        html.push_str(value);
        rest = after;
    }
    html.push_str(rest);
    Ok(html)
}

/// Write the standalone interactive HTML report to `output_path`.
pub fn write_html_report(report: &AnalysisReport, output_path: &Path) -> Result<()> {
    write_report(output_path, || render_html_report(report))
}
