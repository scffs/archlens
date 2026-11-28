use std::path::Path;

use crate::domain::metrics::AnalysisReport;
use crate::error::{ArchLensError, Result};
use crate::report::output::write_report;

/// Serialize an `AnalysisReport` to pretty-printed JSON.
pub fn to_json_string(report: &AnalysisReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

/// Write an `AnalysisReport` as JSON to `output_path`.
pub fn write_json_report(report: &AnalysisReport, output_path: &Path) -> Result<()> {
    write_report(output_path, || to_json_string(report))
}

/// Load a previously saved `AnalysisReport` JSON snapshot from disk.
pub fn load_report_json(path: &Path) -> Result<AnalysisReport> {
    let raw = std::fs::read_to_string(path).map_err(|source| ArchLensError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(serde_json::from_str(&raw)?)
}
