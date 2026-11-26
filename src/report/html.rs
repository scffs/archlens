use std::path::Path;

use crate::domain::metrics::AnalysisReport;
use crate::error::{ArchLensError, Result};

const HTML_SHELL: &str = include_str!("shell.html");
const STYLE_CSS: &str = include_str!("assets/style.css");
const MODEL_JS: &str = include_str!("assets/model.js");
const LAYOUT_JS: &str = include_str!("assets/layout.js");
const GRAPH_JS: &str = include_str!("assets/graph.js");
const DSM_JS: &str = include_str!("assets/dsm.js");
const CHARTS_JS: &str = include_str!("assets/charts.js");
const INSPECTOR_JS: &str = include_str!("assets/inspector.js");
const APP_JS: &str = include_str!("assets/app.js");

/// Render a fully self-contained interactive HTML report (no CDN or server required).
pub fn render_html_report(report: &AnalysisReport) -> Result<String> {
    let json = serde_json::to_string(report)?;
    // Prevent `</script>` sequences inside serialized file paths from prematurely closing the script tag.
    let safe_json = json.replace("</", "<\\/");

    Ok(HTML_SHELL
        .replace("__ARCHLENS_STYLE_CSS__", STYLE_CSS)
        .replace("__ARCHLENS_REPORT_JSON__", &safe_json)
        .replace("__ARCHLENS_MODEL_JS__", MODEL_JS)
        .replace("__ARCHLENS_LAYOUT_JS__", LAYOUT_JS)
        .replace("__ARCHLENS_GRAPH_JS__", GRAPH_JS)
        .replace("__ARCHLENS_DSM_JS__", DSM_JS)
        .replace("__ARCHLENS_CHARTS_JS__", CHARTS_JS)
        .replace("__ARCHLENS_INSPECTOR_JS__", INSPECTOR_JS)
        .replace("__ARCHLENS_APP_JS__", APP_JS))
}

/// Write the standalone interactive HTML report to `output_path`.
pub fn write_html_report(report: &AnalysisReport, output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|source| ArchLensError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let html = render_html_report(report)?;
    std::fs::write(output_path, html).map_err(|source| ArchLensError::Io {
        path: output_path.to_path_buf(),
        source,
    })
}
