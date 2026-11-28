use std::path::Path;

use crate::error::{ArchLensError, Result};

pub(super) fn write_report(
    output_path: &Path,
    render: impl FnOnce() -> Result<String>,
) -> Result<()> {
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|source| ArchLensError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    std::fs::write(output_path, render()?).map_err(|source| ArchLensError::Io {
        path: output_path.to_path_buf(),
        source,
    })
}
