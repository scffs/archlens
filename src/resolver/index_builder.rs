use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use ignore::WalkBuilder;

use crate::domain::model::{ImportKind, Language, ParsedFile};
use crate::paths::{join_posix, normalize_rel_path, parent_dir};
use crate::resolver::index::{ProjectIndex, ResolveOutcome};
use crate::resolver::{go, js_ts, python, rust};

impl ProjectIndex {
    /// Build a repository index from `root` and the list of parsed source files.
    pub fn build(root: &Path, parsed_files: &[ParsedFile]) -> Self {
        Self::build_for_files(root, parsed_files.iter())
    }

    pub(super) fn build_for_files<'a>(
        root: &Path,
        parsed_files: impl Iterator<Item = &'a ParsedFile> + Clone,
    ) -> Self {
        let mut index = Self::default();

        for pf in parsed_files.clone() {
            index.known_files.insert(pf.path.clone());
            let dir = parent_dir(&pf.path);
            index
                .dir_files
                .entry(dir)
                .or_default()
                .push(pf.path.clone());

            let decl_set: HashSet<String> =
                pf.declarations.iter().map(|d| d.name.clone()).collect();
            let abs_set: HashSet<String> = pf
                .declarations
                .iter()
                .filter(|d| d.is_abstract)
                .map(|d| d.name.clone())
                .collect();
            index.file_decls.insert(pf.path.clone(), decl_set);
            index.abstract_decls.insert(pf.path.clone(), abs_set);
        }
        for files in index.dir_files.values_mut() {
            files.sort();
        }

        let mut py_roots = BTreeSet::new();
        py_roots.insert(String::new());

        let walker = WalkBuilder::new(root)
            .standard_filters(true)
            .hidden(true)
            .build();

        for entry_result in walker {
            let Ok(entry) = entry_result else {
                continue;
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            let rel_path = normalize_rel_path(root, path);
            if rel_path.contains("/node_modules/")
                || rel_path.starts_with("node_modules/")
                || rel_path.contains("/target/")
                || rel_path.starts_with("target/")
            {
                continue;
            }
            let dir = parent_dir(&rel_path);

            match file_name {
                "Cargo.toml" => {
                    if let Some((manifest, crate_info)) =
                        rust::load_cargo_manifest(path, &dir, &index.known_files)
                    {
                        index.packages.push(manifest);
                        index
                            .rust_crates
                            .insert(crate_info.crate_ident.clone(), crate_info.clone());
                        index.rust_crates_by_dir.push(crate_info);
                    }
                }
                "package.json" => {
                    if let Some((manifest, js_pkg)) = js_ts::load_package_json(path, &dir) {
                        index.packages.push(manifest);
                        index.js_packages.insert(js_pkg.name.clone(), js_pkg);
                    }
                }
                "tsconfig.json" | "jsconfig.json" | "tsconfig.app.json" => {
                    if let Some(tsconfig) = js_ts::load_tsconfig(root, path, &dir) {
                        index.tsconfigs.push(tsconfig);
                    }
                }
                "pyproject.toml" => {
                    if let Some(manifest) = python::load_pyproject(path, &dir) {
                        index.packages.push(manifest);
                    }
                    py_roots.insert(dir.clone());
                    let src_sub = join_posix(&dir, "src");
                    py_roots.insert(src_sub);
                }
                "go.mod" => {
                    if let Some((manifest, go_mod)) = go::load_go_mod(path, &dir) {
                        index.packages.push(manifest);
                        index.go_modules.push(go_mod);
                    }
                }
                _ => {}
            }
        }

        for pf in parsed_files.clone() {
            if pf.language == Language::Python {
                if let Some(idx) = pf.path.find("/src/") {
                    py_roots.insert(pf.path[..idx + 4].to_string());
                } else if pf.path.starts_with("src/") {
                    py_roots.insert("src".to_string());
                }
            }
        }

        index.packages.sort_by(|a, b| {
            b.dir
                .len()
                .cmp(&a.dir.len())
                .then_with(|| a.dir.cmp(&b.dir))
        });
        index.rust_crates_by_dir.sort_by(|a, b| {
            b.dir
                .len()
                .cmp(&a.dir.len())
                .then_with(|| a.dir.cmp(&b.dir))
        });
        index.tsconfigs.sort_by(|a, b| {
            b.dir
                .len()
                .cmp(&a.dir.len())
                .then_with(|| b.paths.len().cmp(&a.paths.len()))
        });
        index
            .go_modules
            .sort_by_key(|m| std::cmp::Reverse(m.module_path.len()));
        index.python_roots = py_roots.into_iter().collect();

        // Populate re-export index for Rust (`pub use`) and TS/JS (`export ... from`).
        index.build_reexport_table(parsed_files);

        index
    }

    fn build_reexport_table<'a>(&mut self, parsed_files: impl Iterator<Item = &'a ParsedFile>) {
        let mut direct = Vec::new();
        let mut wildcards = Vec::new();

        for pf in parsed_files {
            for raw in &pf.raw_imports {
                if raw.kind != ImportKind::ReExport {
                    continue;
                }
                let outcome = match pf.language {
                    Language::Rust => rust::resolve_rust_import_direct(self, &pf.path, raw),
                    Language::TypeScript | Language::JavaScript | Language::Vue => {
                        js_ts::resolve_js_ts_import(self, &pf.path, &raw.specifier)
                    }
                    _ => continue,
                };
                if let ResolveOutcome::Internal(target) = outcome
                    && target != pf.path
                {
                    if raw.specifier.ends_with("::*") || raw.symbols.is_empty() {
                        wildcards.push((pf.path.clone(), target));
                    } else {
                        for sym in &raw.symbols {
                            direct.push(((pf.path.clone(), sym.clone()), target.clone()));
                        }
                    }
                }
            }
        }

        for (k, v) in direct {
            self.reexports.insert(k, v);
        }
        for (src, tgt) in wildcards {
            self.wildcard_reexports.entry(src).or_default().push(tgt);
        }
    }
}
