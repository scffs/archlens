use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::domain::model::{FileGitMetrics, FileGraph};

const MAX_FILES_PER_COCHANGE_COMMIT: usize = 25;
const MIN_COCHANGE_COMMITS: usize = 3;
const MIN_TEMPORAL_COUPLING_STRENGTH: f64 = 0.35;
const MAX_TEMPORAL_COUPLINGS: usize = 80;

/// Pair of files that frequently change together in the same Git commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalCoupling {
    pub file_a: String,
    pub file_b: String,
    pub module_a: String,
    pub module_b: String,
    pub shared_commits: usize,
    pub commits_a: usize,
    pub commits_b: usize,
    /// Symmetric coupling strength `2 * shared / (commits_a + commits_b)` in `0.0..=1.0`.
    pub strength: f64,
    /// True if a direct static import edge exists between `file_a` and `file_b`.
    pub has_static_edge: bool,
    /// True if `file_a` and `file_b` belong to different architectural modules.
    pub cross_module: bool,
}

/// Repository-level Git behavioral analysis metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitAnalysis {
    pub lookback_days: u32,
    pub commits_analyzed: usize,
    pub active_files: usize,
    pub temporal_couplings: Vec<TemporalCoupling>,
}

struct FileAccumulator {
    commits: usize,
    lines_added: usize,
    lines_deleted: usize,
    authors: HashMap<String, usize>,
    last_ts: u64,
}

/// Enrich `graph.files` with Git churn/hotspot/ownership metrics and return temporal couplings.
///
/// Returns `None` cleanly if `root` is not inside a Git repository or `git` is unavailable.
pub fn analyze_git_history(
    root: &Path,
    graph: &mut FileGraph,
    lookback_days: u32,
    max_commits: usize,
) -> Option<GitAnalysis> {
    let toplevel = git_toplevel(root)?;
    let canonical_root = root.canonicalize().ok()?;
    let canonical_top = Path::new(&toplevel).canonicalize().ok()?;
    let rel_prefix = canonical_root
        .strip_prefix(&canonical_top)
        .ok()
        .map_or_default(|p| {
            p.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        });

    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(&canonical_top)
        .arg("log")
        .arg("--no-merges")
        .arg(format!("--since={lookback_days}.days.ago"))
        .arg(format!("-n{max_commits}"))
        .arg("--format====COMMIT===%ct\t%aN")
        .arg("--numstat");

    if !rel_prefix.is_empty() {
        cmd.arg("--").arg(&rel_prefix);
    }

    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let known_files: HashSet<&str> = graph.files.iter().map(|f| f.path.as_str()).collect();
    let module_by_file: HashMap<&str, &str> = graph
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.module_id.as_str()))
        .collect();

    let mut static_pairs: HashSet<(&str, &str)> = HashSet::with_capacity(graph.edges.len());
    for e in &graph.edges {
        let pair = ordered_pair(e.from.as_str(), e.to.as_str());
        static_pairs.insert(pair);
    }

    let mut acc: HashMap<String, FileAccumulator> = HashMap::new();
    let mut cochanges: HashMap<(String, String), usize> = HashMap::new();
    let mut commits_analyzed = 0usize;
    let mut latest_ts = 0u64;

    let mut cur_ts = 0u64;
    let mut cur_author = String::new();
    let mut cur_files: Vec<String> = Vec::new();

    let flush_commit =
        |files: &mut Vec<String>, cochanges: &mut HashMap<(String, String), usize>| {
            files.sort();
            files.dedup();
            if files.len() >= 2 && files.len() <= MAX_FILES_PER_COCHANGE_COMMIT {
                for i in 0..files.len() {
                    for j in (i + 1)..files.len() {
                        *cochanges
                            .entry((files[i].clone(), files[j].clone()))
                            .or_insert(0) += 1;
                    }
                }
            }
            files.clear();
        };

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("===COMMIT===") {
            flush_commit(&mut cur_files, &mut cochanges);
            commits_analyzed += 1;
            let mut parts = rest.splitn(2, '\t');
            cur_ts = parts
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            cur_author = parts.next().unwrap_or("unknown").trim().to_string();
            if cur_ts > latest_ts {
                latest_ts = cur_ts;
            }
            continue;
        }

        if line.trim().is_empty() {
            continue;
        }

        let mut cols = line.splitn(3, '\t');
        let (Some(add_s), Some(del_s), Some(raw_path)) = (cols.next(), cols.next(), cols.next())
        else {
            continue;
        };

        let normalized_path = normalize_git_numstat_path(raw_path);
        let rel_to_root = if rel_prefix.is_empty() {
            normalized_path
        } else if let Some(stripped) = normalized_path.strip_prefix(&format!("{rel_prefix}/")) {
            stripped.to_string()
        } else {
            continue;
        };

        if !known_files.contains(rel_to_root.as_str()) {
            continue;
        }

        let added = add_s.parse::<usize>().unwrap_or(0);
        let deleted = del_s.parse::<usize>().unwrap_or(0);

        let entry = acc.entry(rel_to_root.clone()).or_insert_with(|| FileAccumulator {
            commits: 0,
            lines_added: 0,
            lines_deleted: 0,
            authors: HashMap::new(),
            last_ts: 0,
        });
        entry.commits += 1;
        entry.lines_added += added;
        entry.lines_deleted += deleted;
        *entry.authors.entry(cur_author.clone()).or_insert(0) += 1;
        if cur_ts > entry.last_ts {
            entry.last_ts = cur_ts;
        }
        cur_files.push(rel_to_root);
    }
    flush_commit(&mut cur_files, &mut cochanges);

    if commits_analyzed == 0 {
        return None;
    }

    // Compute per-file degree for hotspot weighting.
    let mut degree: HashMap<&str, usize> = HashMap::new();
    for e in &graph.edges {
        *degree.entry(e.from.as_str()).or_insert(0) += 1;
        *degree.entry(e.to.as_str()).or_insert(0) += 1;
    }

    let mut raw_hotspots: Vec<(usize, f64)> = Vec::new();
    let mut max_raw_hotspot = 0.0_f64;

    for (idx, file) in graph.files.iter().enumerate() {
        if let Some(a) = acc.get(&file.path) {
            let deg = degree.get(file.path.as_str()).copied().unwrap_or(0);
            let structural = (file.loc as f64).ln_1p() * (1.0 + 0.15 * deg as f64);
            let raw = structural * (a.commits as f64).ln_1p();
            if raw > max_raw_hotspot {
                max_raw_hotspot = raw;
            }
            raw_hotspots.push((idx, raw));
        }
    }

    let hotspot_map: HashMap<usize, f64> = raw_hotspots
        .into_iter()
        .map(|(idx, raw)| {
            let norm = if max_raw_hotspot > 0.0 {
                round4(raw / max_raw_hotspot)
            } else {
                0.0
            };
            (idx, norm)
        })
        .collect();

    // Build temporal couplings before mutating `graph.files`.
    let mut temporal_couplings: Vec<TemporalCoupling> = cochanges
        .into_iter()
        .filter_map(|((file_a, file_b), shared)| {
            if shared < MIN_COCHANGE_COMMITS {
                return None;
            }
            let commits_a = acc.get(&file_a)?.commits;
            let commits_b = acc.get(&file_b)?.commits;
            let strength = (2.0 * shared as f64) / (commits_a + commits_b) as f64;
            if strength < MIN_TEMPORAL_COUPLING_STRENGTH {
                return None;
            }
            let module_a = module_by_file
                .get(file_a.as_str())
                .copied()
                .unwrap_or("")
                .to_string();
            let module_b = module_by_file
                .get(file_b.as_str())
                .copied()
                .unwrap_or("")
                .to_string();
            let has_static_edge =
                static_pairs.contains(&ordered_pair(file_a.as_str(), file_b.as_str()));
            let cross_module = module_a != module_b;

            Some(TemporalCoupling {
                file_a,
                file_b,
                module_a,
                module_b,
                shared_commits: shared,
                commits_a,
                commits_b,
                strength: round4(strength),
                has_static_edge,
                cross_module,
            })
        })
        .collect();

    // Prioritize hidden cross-module couplings first, then highest strength and shared count.
    temporal_couplings.sort_by(|a, b| {
        let a_hidden_cross = !a.has_static_edge && a.cross_module;
        let b_hidden_cross = !b.has_static_edge && b.cross_module;
        b_hidden_cross
            .cmp(&a_hidden_cross)
            .then_with(|| (!b.has_static_edge).cmp(&(!a.has_static_edge)))
            .then_with(|| b.strength.total_cmp(&a.strength))
            .then_with(|| b.shared_commits.cmp(&a.shared_commits))
            .then_with(|| a.file_a.cmp(&b.file_a))
    });
    temporal_couplings.truncate(MAX_TEMPORAL_COUPLINGS);

    let active_files = acc.len();
    for (idx, file) in graph.files.iter_mut().enumerate() {
        if let Some(a) = acc.get(&file.path) {
            let (top_author, top_count) = a
                .authors
                .iter()
                .max_by(|(name_a, cnt_a), (name_b, cnt_b)| {
                    cnt_a.cmp(cnt_b).then_with(|| name_b.cmp(name_a))
                })
                .map_or_else(
                    || ("unknown".to_string(), 0usize),
                    |(name, &cnt)| (name.clone(), cnt),
                );
            let top_author_share = if a.commits > 0 {
                round4(top_count as f64 / a.commits as f64)
            } else {
                0.0
            };
            let days_since_last_change = latest_ts.saturating_sub(a.last_ts) / 86_400;

            file.git = Some(FileGitMetrics {
                commits: a.commits,
                lines_added: a.lines_added,
                lines_deleted: a.lines_deleted,
                authors: a.authors.len(),
                top_author_share,
                top_author,
                days_since_last_change,
                hotspot_score: hotspot_map.get(&idx).copied().unwrap_or(0.0),
            });
        }
    }

    Some(GitAnalysis {
        lookback_days,
        commits_analyzed,
        active_files,
        temporal_couplings,
    })
}

fn git_toplevel(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!path.is_empty()).then_some(path)
}

/// Handle Git rename notation like `dir/{old.rs => new.rs}` or `old.rs => new.rs`.
fn normalize_git_numstat_path(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('"');
    if let (Some(open), Some(close)) = (trimmed.find('{'), trimmed.find('}'))
        && open < close
    {
        let inside = &trimmed[open + 1..close];
        if let Some((_, new_part)) = inside.split_once(" => ") {
            let combined = format!("{}{}{}", &trimmed[..open], new_part, &trimmed[close + 1..]);
            return combined.replace("//", "/");
        }
    }
    if let Some((_, new_part)) = trimmed.split_once(" => ") {
        return new_part.trim().to_string();
    }
    trimmed.to_string()
}

fn ordered_pair<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    if a <= b { (a, b) } else { (b, a) }
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
