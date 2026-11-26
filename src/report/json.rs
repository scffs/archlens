use std::path::Path;

use crate::domain::metrics::AnalysisReport;
use crate::error::{ArchLensError, Result};

/// Serialize an `AnalysisReport` to pretty-printed JSON.
pub fn to_json_string(report: &AnalysisReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

/// Write an `AnalysisReport` as JSON to `output_path`.
pub fn write_json_report(report: &AnalysisReport, output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|source| ArchLensError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let payload = to_json_string(report)?;
    std::fs::write(output_path, payload).map_err(|source| ArchLensError::Io {
        path: output_path.to_path_buf(),
        source,
    })
}
