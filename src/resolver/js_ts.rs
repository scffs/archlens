pub(super) mod imports;

pub use self::imports::{AliasFallback, resolve_js_ts_import_with_aliases};
pub use crate::resolver::web_defaults::resolve_js_ts_import;

use std::path::Path;

use serde_json::Value;

use crate::paths::{join_posix, normalize_rel_path};
use crate::resolver::index::{JsPackageInfo, PackageManifest, TsConfigInfo};

/// Load a `package.json` manifest for monorepo package detection and cross-package resolution.
pub fn load_package_json(abs_path: &Path, dir: &str) -> Option<(PackageManifest, JsPackageInfo)> {
    let raw = std::fs::read_to_string(abs_path).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let name = json.get("name")?.as_str()?.to_string();

    let mut entry_candidates = Vec::new();

    if let Some(exports) = json.get("exports") {
        collect_export_entries(exports, &mut entry_candidates);
    }
    for key in ["source", "module", "main", "types"] {
        if let Some(val) = json.get(key).and_then(Value::as_str) {
            entry_candidates.push(val.to_string());
        }
    }
    entry_candidates.extend([
        "src/index.ts".to_string(),
        "src/index.tsx".to_string(),
        "src/index.js".to_string(),
        "src/main.ts".to_string(),
        "index.ts".to_string(),
        "index.js".to_string(),
    ]);

    let manifest = PackageManifest {
        name: name.clone(),
        dir: dir.to_string(),
    };
    let info = JsPackageInfo {
        name,
        dir: dir.to_string(),
        entry_candidates,
    };

    Some((manifest, info))
}

fn collect_export_entries(exports: &Value, out: &mut Vec<String>) {
    match exports {
        Value::String(s) => out.push(s.clone()),
        Value::Object(map) => {
            if let Some(dot) = map.get(".") {
                collect_export_entries(dot, out);
            } else {
                for key in ["import", "default", "types", "require"] {
                    if let Some(v) = map.get(key) {
                        collect_export_entries(v, out);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Load a `tsconfig.json` / `jsconfig.json`, supporting JSONC, `extends`, and project `references`
/// (including generated configs explicitly referenced by the project).
pub fn load_tsconfig(root: &Path, abs_path: &Path, dir: &str) -> Option<TsConfigInfo> {
    let mut base_dir: Option<String> = None;
    let mut paths: Vec<(String, Vec<String>)> = Vec::new();
    load_tsconfig_chain(root, abs_path, 0, &mut base_dir, &mut paths);

    let has_base_url = base_dir.is_some();
    let resolved_base = base_dir.unwrap_or_else(|| dir.to_string());

    if !has_base_url && paths.is_empty() {
        return None;
    }

    Some(TsConfigInfo {
        dir: dir.to_string(),
        base_dir: resolved_base,
        paths,
        has_base_url,
    })
}

fn load_tsconfig_chain(
    root: &Path,
    abs_path: &Path,
    depth: usize,
    base_dir: &mut Option<String>,
    paths: &mut Vec<(String, Vec<String>)>,
) {
    if depth > 6 {
        return;
    }
    let Ok(raw) = std::fs::read_to_string(abs_path) else {
        return;
    };
    let cleaned = strip_jsonc(&raw);
    let Ok(json) = serde_json::from_str::<Value>(&cleaned) else {
        return;
    };

    let cfg_dir = abs_path
        .parent()
        .map_or_else(String::new, |p| normalize_rel_path(root, p));

    // Follow `extends` chain.
    if let Some(extends_val) = json.get("extends").and_then(Value::as_str)
        && extends_val.starts_with('.')
        && let Some(parent) = abs_path.parent()
    {
        let mut target = parent.join(extends_val);
        if !target.is_file() && target.extension().is_none() {
            target.set_extension("json");
        }
        if target.starts_with(root) && target.is_file() {
            load_tsconfig_chain(root, &target, depth + 1, base_dir, paths);
        }
    }

    // Follow explicitly referenced configs, including generated files.
    if let Some(refs) = json.get("references").and_then(Value::as_array)
        && let Some(parent) = abs_path.parent()
    {
        for r in refs {
            if let Some(ref_path) = r.get("path").and_then(Value::as_str) {
                let mut target = parent.join(ref_path);
                if target.is_dir() {
                    target = target.join("tsconfig.json");
                } else if !target.is_file() && target.extension().is_none() {
                    target.set_extension("json");
                }
                if target.starts_with(root) && target.is_file() {
                    let mut ref_paths = Vec::new();
                    load_tsconfig_chain(root, &target, depth + 1, &mut None, &mut ref_paths);
                    for (pattern, targets) in ref_paths {
                        // The referenced config has already made its targets repository-relative.
                        if !paths.iter().any(|(existing, _)| existing == &pattern) {
                            paths.push((pattern, targets));
                        }
                    }
                }
            }
        }
    }

    if let Some(opts) = json.get("compilerOptions").and_then(Value::as_object) {
        let local_base = if let Some(bu) = opts.get("baseUrl").and_then(Value::as_str) {
            let joined = join_posix(&cfg_dir, bu);
            *base_dir = Some(joined.clone());
            joined
        } else {
            base_dir.clone().unwrap_or_else(|| cfg_dir.clone())
        };

        if let Some(paths_obj) = opts.get("paths").and_then(Value::as_object) {
            for (pattern, targets_val) in paths_obj {
                if let Some(arr) = targets_val.as_array() {
                    let targets: Vec<String> = arr
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|rep| join_posix(&local_base, rep))
                        .collect();
                    if !targets.is_empty() {
                        paths.retain(|(existing, _)| existing != pattern);
                        paths.push((pattern.clone(), targets));
                    }
                }
            }
        }
    }
}

/// Strip `//` line comments, `/* ... */` block comments, and trailing commas from JSONC text.
fn strip_jsonc(input: &str) -> String {
    let mut no_comments = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            no_comments.push(b);
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if b == b'"' {
            in_string = true;
            no_comments.push(b'"');
            i += 1;
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else {
            no_comments.push(b);
            i += 1;
        }
    }

    let no_comments =
        String::from_utf8(no_comments).expect("stripping ASCII comment delimiters preserves UTF-8");
    let chars: Vec<char> = no_comments.chars().collect();
    let mut out = String::with_capacity(no_comments.len());
    let mut idx = 0;
    let mut inside_str = false;
    let mut esc = false;

    while idx < chars.len() {
        let ch = chars[idx];
        if inside_str {
            out.push(ch);
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                inside_str = false;
            }
            idx += 1;
            continue;
        }
        if ch == '"' {
            inside_str = true;
            out.push(ch);
            idx += 1;
            continue;
        }
        if ch == ',' {
            let mut look = idx + 1;
            while look < chars.len() && chars[look].is_whitespace() {
                look += 1;
            }
            if look < chars.len() && matches!(chars[look], '}' | ']') {
                idx += 1;
                continue;
            }
        }
        out.push(ch);
        idx += 1;
    }

    out
}
