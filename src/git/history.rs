use std::borrow::Cow;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

use crate::paths::normalize_rel_path;

const COMMIT_MARKER: &str = "archlens-commit";

pub(super) enum HistoryRecord<'a> {
    Commit {
        timestamp: u64,
        author: Cow<'a, str>,
    },
    File {
        path: Cow<'a, str>,
        added: usize,
        deleted: usize,
    },
}

pub(super) struct HistoryLog {
    stdout: Vec<u8>,
    path_prefix: String,
}

impl HistoryLog {
    pub(super) fn read(root: &Path, lookback_days: u32, max_commits: usize) -> Option<Self> {
        let toplevel = git_toplevel(root)?;
        let canonical_root = root.canonicalize().ok()?;
        let canonical_top = Path::new(&toplevel).canonicalize().ok()?;
        let relative_root = if canonical_root.starts_with(&canonical_top) {
            normalize_rel_path(&canonical_top, &canonical_root)
        } else {
            String::new()
        };

        let mut cmd = Command::new("git");
        cmd.arg("--literal-pathspecs")
            .arg("-C")
            .arg(&canonical_top)
            .arg("log")
            .arg("--no-merges")
            .arg("--no-show-signature")
            .arg(format!("--since={lookback_days}.days.ago"))
            .arg(format!("-n{max_commits}"))
            .arg(format!("--format={COMMIT_MARKER}%x00%ct%x00%aN"))
            .args(["--numstat", "-z"]);

        let path_prefix = if relative_root.is_empty() {
            String::new()
        } else {
            cmd.arg("--").arg(&relative_root);
            format!("{relative_root}/")
        };
        let output = cmd.output().ok()?;
        output.status.success().then_some(Self {
            stdout: output.stdout,
            path_prefix,
        })
    }

    pub(super) fn records(&self) -> impl Iterator<Item = HistoryRecord<'_>> {
        let mut fields = self.stdout.split(|&byte| byte == 0);
        std::iter::from_fn(move || {
            loop {
                let field = fields.next()?;
                let field = field.strip_prefix(b"\n").unwrap_or(field);
                if field == COMMIT_MARKER.as_bytes() {
                    let timestamp = parse_number(fields.next()?);
                    let author = String::from_utf8_lossy(fields.next()?);
                    return Some(HistoryRecord::Commit { timestamp, author });
                }

                let mut columns = field.splitn(3, |&byte| byte == b'\t');
                let (Some(added), Some(deleted), Some(path)) =
                    (columns.next(), columns.next(), columns.next())
                else {
                    continue;
                };
                let path = if path.is_empty() {
                    // A rename or copy has an empty path field followed by the source and destination.
                    fields.next()?;
                    fields.next()?
                } else {
                    path
                };
                let Some(path) = path.strip_prefix(self.path_prefix.as_bytes()) else {
                    continue;
                };
                return Some(HistoryRecord::File {
                    path: String::from_utf8_lossy(path),
                    added: parse_number(added),
                    deleted: parse_number(deleted),
                });
            }
        })
    }
}

fn parse_number<T: Default + FromStr>(bytes: &[u8]) -> T {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or_default()
}

fn git_toplevel(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = output.stdout.strip_suffix(b"\n").unwrap_or(&output.stdout);
    #[cfg(windows)]
    let path = path.strip_suffix(b"\r").unwrap_or(path);
    (!path.is_empty()).then(|| String::from_utf8_lossy(path).into_owned())
}
