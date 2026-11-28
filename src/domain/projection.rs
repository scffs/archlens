use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use globset::GlobSet;

use crate::config::{AnalysisConfig, ComponentRule};
use crate::domain::model::{FileGraph, FileNode};
use crate::error::Result;
use crate::paths::build_glob_set;

const MAX_SEPARATE_ROOT_MODULES: usize = 10;
const COMPONENTS_VIEW: &str = "components";
const MODULES_VIEW: &str = "modules";
const PACKAGES_VIEW: &str = "packages";
const FILES_VIEW: &str = "files";
const FOLDER_VIEW_PREFIX: &str = "folders-d";

/// A planned grouping of files into logical architectural nodes.
pub struct ViewProjection {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Mapping `file_path -> group_id`.
    pub file_to_group: HashMap<String, String>,
}

/// Supplies an additional projection of the analyzed file graph.
pub trait ViewProvider {
    /// Produce a view for this graph, or skip it by returning `None`.
    fn project(&self, graph: &FileGraph, config: &AnalysisConfig)
    -> Result<Option<ViewProjection>>;
}

struct CompiledComponentRule {
    name: String,
    matcher: GlobSet,
}

/// Compute and populate `module_id` on every `FileNode` in `graph`.
pub fn assign_auto_module_ids(graph: &mut FileGraph) {
    let mapping = compute_auto_module_map(&graph.files);
    for file in &mut graph.files {
        if let Some(id) = mapping.get(&file.path) {
            file.module_id.clone_from(id);
        }
    }
}

/// Build all standard architecture views (components/modules, packages, folder depths, files).
pub fn build_projections(
    graph: &FileGraph,
    config: &AnalysisConfig,
) -> Result<Vec<ViewProjection>> {
    build_projections_with(graph, config, &[])
}

pub(crate) fn build_projections_with(
    graph: &FileGraph,
    config: &AnalysisConfig,
    providers: &[&dyn ViewProvider],
) -> Result<Vec<ViewProjection>> {
    let mut views = Vec::new();

    if !config.components.is_empty() {
        views.push(project_custom_components(graph, &config.components)?);
    }

    views.push(project_auto_modules(graph));

    if let Some(packages_view) = project_packages(graph) {
        views.push(packages_view);
    }

    let mut depths: BTreeSet<usize> = config
        .folder_depths
        .iter()
        .copied()
        .filter(|&d| d > 0)
        .collect();
    if depths.is_empty() {
        depths.insert(1);
        depths.insert(2);
    }
    for depth in depths {
        views.push(project_folders(graph, depth));
    }

    views.push(project_files(graph));
    for provider in providers {
        if let Some(mut view) = provider.project(graph, config)?
            && !view.id.is_empty()
            && !is_builtin_view_id(&view.id)
            && !views.iter().any(|existing| existing.id == view.id)
        {
            for file in &graph.files {
                view.file_to_group
                    .entry(file.path.clone())
                    .or_insert_with(|| file.path.clone());
            }
            views.push(view);
        }
    }

    Ok(views)
}

fn is_builtin_view_id(id: &str) -> bool {
    matches!(
        id,
        COMPONENTS_VIEW | MODULES_VIEW | PACKAGES_VIEW | FILES_VIEW
    ) || id.starts_with(FOLDER_VIEW_PREFIX)
}

fn project_custom_components(graph: &FileGraph, rules: &[ComponentRule]) -> Result<ViewProjection> {
    let mut compiled = Vec::with_capacity(rules.len());
    for rule in rules {
        compiled.push(CompiledComponentRule {
            name: rule.name.clone(),
            matcher: build_glob_set(&rule.patterns)?,
        });
    }

    let mut file_to_group = HashMap::with_capacity(graph.files.len());
    for file in &graph.files {
        let group = compiled
            .iter()
            .find(|r| r.matcher.is_match(&file.path))
            .map_or_else(|| "<unassigned>".to_string(), |r| r.name.clone());
        file_to_group.insert(file.path.clone(), group);
    }

    Ok(ViewProjection {
        id: COMPONENTS_VIEW.to_string(),
        title: "Components (configured)".to_string(),
        description: "Logical components defined via archlens.toml rules.".to_string(),
        file_to_group,
    })
}

/// Smart language-aware module grouping:
/// - Flat packages with no subdirectories under `src/` stay as `<pkg>` (no artificial `/src` bucket).
/// - Rust 2018 module roots (`foo.rs` paired with `foo/`) merge into `<pkg>/foo`.
/// - Root composition roots (`main.rs`, `di.rs`, `lib.rs`) and top-level root modules (`errors.rs`,
///   `config.rs`, etc.) stay distinct so they don't form artificial cycles with feature modules.
fn project_auto_modules(graph: &FileGraph) -> ViewProjection {
    let file_to_group = compute_auto_module_map(&graph.files);
    ViewProjection {
        id: MODULES_VIEW.to_string(),
        title: "Modules (auto)".to_string(),
        description:
            "Language-aware top-level modules (pairs Rust 2018 foo.rs + foo/, separates composition roots)."
                .to_string(),
        file_to_group,
    }
}

struct PackageLayoutInfo {
    subdirs: HashSet<String>,
    loose_root_files: usize,
}

fn compute_auto_module_map(files: &[FileNode]) -> HashMap<String, String> {
    let distinct_packages: BTreeSet<&str> =
        files.iter().filter_map(|f| f.package.as_deref()).collect();
    let multi_package = distinct_packages.len() > 1;

    // Pass 1: discover subdirectories and loose root files per package.
    let mut pkg_info: HashMap<String, PackageLayoutInfo> = HashMap::new();
    for file in files {
        let pkg_key = file.package.clone().unwrap_or_default();
        let entry = pkg_info
            .entry(pkg_key)
            .or_insert_with(|| PackageLayoutInfo {
                subdirs: HashSet::new(),
                loose_root_files: 0,
            });
        let (first_subdir, _) = split_source_relative(file);
        if let Some(sub) = first_subdir {
            entry.subdirs.insert(sub.to_string());
        }
    }

    // Count non-paired loose root files per package.
    for file in files {
        let pkg_key = file.package.clone().unwrap_or_default();
        let (first_subdir, file_stem) = split_source_relative(file);
        if first_subdir.is_none()
            && let Some(info) = pkg_info.get_mut(&pkg_key)
            && !info.subdirs.contains(file_stem)
        {
            info.loose_root_files += 1;
        }
    }

    // Pass 2: assign each file its clean module key.
    let mut file_to_group = HashMap::with_capacity(files.len());
    for file in files {
        let pkg_key = file.package.clone().unwrap_or_default();
        let info = &pkg_info[&pkg_key];
        let (first_subdir, file_stem) = split_source_relative(file);

        let sub_module: Option<String> = if let Some(sub) = first_subdir {
            Some(sub.to_string())
        } else if info.subdirs.contains(file_stem) {
            // Rust 2018 / JS barrel pairing: `application.rs` + `application/*` -> `application`.
            Some(file_stem.to_string())
        } else if info.subdirs.is_empty() {
            // Flat package (e.g. `moex-utils`, `moex-migration`) — keep whole package as one module.
            None
        } else if info.loose_root_files <= MAX_SEPARATE_ROOT_MODULES
            || is_entrypoint_stem(file_stem)
        {
            // Keep composition roots (`main`, `di`, `lib`) and top-level modules (`errors`, `extractors`) distinct.
            Some(file_stem.to_string())
        } else {
            Some("(root)".to_string())
        };

        let group = if multi_package {
            if let Some(pkg) = file.package.as_deref() {
                match sub_module {
                    Some(sub) => format!("{pkg}/{sub}"),
                    None => pkg.to_string(),
                }
            } else {
                match sub_module {
                    Some(sub) => sub,
                    None => folder_at_depth(&file.path, 1),
                }
            }
        } else {
            match sub_module {
                Some(sub) => sub,
                None => file.package.clone().unwrap_or_else(|| "<root>".to_string()),
            }
        };

        file_to_group.insert(file.path.clone(), group);
    }

    file_to_group
}

/// Returns `(first_subdir_under_source_root, file_stem)`.
fn split_source_relative(file: &FileNode) -> (Option<&str>, &str) {
    let rel_to_pkg = match file.package_root.as_deref() {
        Some(root) if !root.is_empty() => file
            .path
            .strip_prefix(root)
            .and_then(|s| s.strip_prefix('/'))
            .unwrap_or(&file.path),
        _ => &file.path,
    };

    let parts: Vec<&str> = rel_to_pkg.split('/').collect();
    let file_name = parts.last().copied().unwrap_or(rel_to_pkg);
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);

    let dir_parts = if parts.len() > 1 {
        &parts[..parts.len() - 1]
    } else {
        &[]
    };

    // Strip standard source container directory (`src`, `lib`, `app`) regardless of whether
    // nested subdirectories exist — so `src/foo.rs` is treated as root-level inside the crate!
    let trimmed = match dir_parts {
        ["src" | "lib" | "app", rest @ ..] => rest,
        other => other,
    };

    (trimmed.first().copied(), stem)
}

fn is_entrypoint_stem(stem: &str) -> bool {
    matches!(
        stem,
        "main" | "lib" | "di" | "app" | "bootstrap" | "index" | "mod" | "__init__" | "__main__"
    )
}

fn project_packages(graph: &FileGraph) -> Option<ViewProjection> {
    let distinct_packages: BTreeSet<String> = graph
        .files
        .iter()
        .map(|f| {
            f.package
                .clone()
                .unwrap_or_else(|| folder_at_depth(&f.path, 1))
        })
        .collect();

    if distinct_packages.len() < 2 {
        return None;
    }

    let mut file_to_group = HashMap::with_capacity(graph.files.len());
    for file in &graph.files {
        let group = file
            .package
            .clone()
            .unwrap_or_else(|| folder_at_depth(&file.path, 1));
        file_to_group.insert(file.path.clone(), group);
    }

    Some(ViewProjection {
        id: PACKAGES_VIEW.to_string(),
        title: "Packages / Crates".to_string(),
        description:
            "Manifest-defined packages (Cargo.toml, package.json, pyproject.toml, go.mod)."
                .to_string(),
        file_to_group,
    })
}

fn project_folders(graph: &FileGraph, depth: usize) -> ViewProjection {
    let mut file_to_group = HashMap::with_capacity(graph.files.len());
    for file in &graph.files {
        file_to_group.insert(file.path.clone(), folder_at_depth(&file.path, depth));
    }

    ViewProjection {
        id: format!("{FOLDER_VIEW_PREFIX}{depth}"),
        title: format!("Folders (depth {depth})"),
        description: format!("Directory hierarchy aggregated up to depth {depth}."),
        file_to_group,
    }
}

fn project_files(graph: &FileGraph) -> ViewProjection {
    let mut file_to_group = HashMap::with_capacity(graph.files.len());
    for file in &graph.files {
        file_to_group.insert(file.path.clone(), file.path.clone());
    }

    ViewProjection {
        id: FILES_VIEW.to_string(),
        title: "Files".to_string(),
        description: "Raw file-level dependency graph.".to_string(),
        file_to_group,
    }
}

fn folder_at_depth(rel_path: &str, depth: usize) -> String {
    let parts: Vec<&str> = rel_path.split('/').collect();
    if parts.len() <= 1 {
        return "<root>".to_string();
    }
    let dir_parts = &parts[..parts.len() - 1];
    let take = depth.min(dir_parts.len());
    dir_parts[..take].join("/")
}
