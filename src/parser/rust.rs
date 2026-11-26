use tree_sitter::{Node, Parser};

use crate::domain::model::{
    ImportKind, Language, ParsedFile, RawImport, SymbolCounts, SymbolDecl, SymbolKind, Visibility,
};
use crate::error::{ArchLensError, Result};
use crate::parser::{LanguageParser, count_non_empty_lines, node_text};

pub struct RustParser;

struct RustWalkState {
    include_tests: bool,
    imports: Vec<RawImport>,
    declarations: Vec<SymbolDecl>,
    cfg_test_mods: Vec<String>,
    implemented_traits: Vec<(String, usize)>,
}

impl LanguageParser for RustParser {
    fn parse(&self, rel_path: &str, source: &str, include_tests: bool) -> Result<ParsedFile> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|err| ArchLensError::ParserInit {
                language: "rust",
                message: err.to_string(),
            })?;

        let mut state = RustWalkState {
            include_tests,
            imports: Vec::new(),
            declarations: Vec::new(),
            cfg_test_mods: Vec::new(),
            implemented_traits: Vec::new(),
        };

        if let Some(tree) = parser.parse(source, None) {
            walk_items(tree.root_node(), source, &mut state);
        }

        let symbols = SymbolCounts::from_declarations(&state.declarations);

        Ok(ParsedFile {
            path: rel_path.to_string(),
            language: Language::Rust,
            loc: count_non_empty_lines(source),
            symbols,
            declarations: state.declarations,
            raw_imports: state.imports,
            cfg_test_mods: state.cfg_test_mods,
            implemented_traits: state.implemented_traits,
        })
    }
}

fn walk_items(container: Node<'_>, source: &str, state: &mut RustWalkState) {
    let mut cursor = container.walk();
    let children: Vec<Node<'_>> = container.children(&mut cursor).collect();

    let mut pending_cfg_test = false;

    for child in children {
        let kind = child.kind();

        if kind == "attribute_item" {
            if is_cfg_test_attr(child, source) {
                pending_cfg_test = true;
            }
            continue;
        }

        if kind == "line_comment" || kind == "block_comment" || kind == "inner_attribute_item" {
            continue;
        }

        let skip_test_item = pending_cfg_test && !state.include_tests;
        pending_cfg_test = false;

        if skip_test_item {
            // Remember external `#[cfg(test)] mod foo;` declarations so the resolver excludes
            // those files and submodules from production analysis.
            if kind == "mod_item"
                && child.child_by_field_name("body").is_none()
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let name = node_text(name_node, source).trim();
                if !name.is_empty() {
                    state.cfg_test_mods.push(name.to_string());
                }
            }
            continue;
        }

        match kind {
            "use_declaration" => collect_use_declaration(child, source, &mut state.imports),
            "mod_item" => handle_mod_item(child, source, state),
            "trait_item" => {
                push_decl(child, source, SymbolKind::Trait, true, &mut state.declarations);
                collect_body_references(child, source, &mut state.imports);
            }
            "struct_item" => {
                push_decl(child, source, SymbolKind::Struct, false, &mut state.declarations);
                collect_body_references(child, source, &mut state.imports);
            }
            "enum_item" | "union_item" => {
                push_decl(child, source, SymbolKind::Enum, false, &mut state.declarations);
                collect_body_references(child, source, &mut state.imports);
            }
            "function_item" => {
                push_decl(child, source, SymbolKind::Function, false, &mut state.declarations);
                collect_body_references(child, source, &mut state.imports);
            }
            "type_item" => {
                let is_abstract = is_abstract_type_alias(child, source);
                if is_abstract {
                    push_decl(
                        child,
                        source,
                        SymbolKind::TypeAlias,
                        true,
                        &mut state.declarations,
                    );
                }
                collect_body_references(child, source, &mut state.imports);
            }
            "impl_item" => {
                handle_impl_item(child, source, state);
            }
            _ => {}
        }
    }
}

fn push_decl(
    node: Node<'_>,
    source: &str,
    kind: SymbolKind,
    is_abstract: bool,
    declarations: &mut Vec<SymbolDecl>,
) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(name_node, source).trim();
    if name.is_empty() {
        return;
    }
    declarations.push(SymbolDecl {
        name: name.to_string(),
        kind,
        visibility: extract_visibility(node, source),
        is_abstract,
        line: node.start_position().row + 1,
    });
}

fn extract_visibility(node: Node<'_>, source: &str) -> Visibility {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            let text = node_text(child, source).trim();
            if text == "pub" {
                return Visibility::Public;
            }
            return Visibility::Internal;
        }
    }
    Visibility::Private
}

fn is_abstract_type_alias(node: Node<'_>, source: &str) -> bool {
    let Some(ty) = node.child_by_field_name("type") else {
        return false;
    };
    let text = node_text(ty, source);
    text.contains("dyn ") || text.contains("impl ") || text.contains("fn(")
}

fn handle_impl_item(node: Node<'_>, source: &str, state: &mut RustWalkState) {
    if let Some(trait_node) = node.child_by_field_name("trait") {
        let mut segs = Vec::new();
        collect_trait_path_segments(trait_node, source, &mut segs);
        if let Some(last) = segs.last() {
            state
                .implemented_traits
                .push((last.clone(), trait_node.start_position().row + 1));
        }
        if matches!(segs.first().map(String::as_str), Some("crate" | "super")) && segs.len() >= 2 {
            let sym = segs.last().cloned().into_iter().collect();
            state.imports.push(RawImport {
                specifier: segs.join("::"),
                kind: ImportKind::TraitImpl,
                line: trait_node.start_position().row + 1,
                symbols: sym,
            });
        }
    }
    collect_body_references(node, source, &mut state.imports);
}

fn collect_trait_path_segments(node: Node<'_>, source: &str, out: &mut Vec<String>) {
    match node.kind() {
        "generic_type" => {
            if let Some(ty) = node.child_by_field_name("type") {
                collect_trait_path_segments(ty, source, out);
            }
        }
        _ => collect_path_segments(node, source, out),
    }
}

fn handle_mod_item(node: Node<'_>, source: &str, state: &mut RustWalkState) {
    if let Some(body) = node.child_by_field_name("body") {
        let mod_name = node
            .child_by_field_name("name")
            .map_or("", |n| node_text(n, source));
        if !state.include_tests && (mod_name == "tests" || mod_name == "test") {
            return;
        }
        walk_items(body, source, state);
    } else if let Some(name_node) = node.child_by_field_name("name") {
        let name = node_text(name_node, source).trim();
        if !name.is_empty() {
            state.imports.push(RawImport {
                specifier: format!("mod::{name}"),
                kind: ImportKind::ModuleDecl,
                line: node.start_position().row + 1,
                symbols: Vec::new(),
            });
        }
    }
}

fn collect_body_references(node: Node<'_>, source: &str, imports: &mut Vec<RawImport>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "use_declaration" => collect_use_declaration(child, source, imports),
            "scoped_identifier" | "scoped_type_identifier" => {
                let mut segs = Vec::new();
                collect_path_segments(child, source, &mut segs);
                if segs.len() >= 2
                    && let Some(first) = segs.first().map(String::as_str)
                    && is_rust_path_candidate(first)
                {
                    let symbols = extract_symbol_hint(&segs);
                    imports.push(RawImport {
                        specifier: segs.join("::"),
                        kind: ImportKind::Import,
                        line: child.start_position().row + 1,
                        symbols,
                    });
                }
            }
            _ if child.child_count() > 0 => collect_body_references(child, source, imports),
            _ => {}
        }
    }
}

fn collect_use_declaration(node: Node<'_>, source: &str, imports: &mut Vec<RawImport>) {
    let mut is_pub = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            is_pub = true;
            break;
        }
    }

    let import_kind = if is_pub {
        ImportKind::ReExport
    } else {
        ImportKind::Import
    };
    let line = node.start_position().row + 1;

    if let Some(arg) = node.child_by_field_name("argument") {
        let mut expanded = Vec::new();
        expand_use_tree(arg, source, &[], &mut expanded);
        for spec in expanded {
            if !spec.is_empty() {
                let segs: Vec<String> = spec.split("::").map(ToString::to_string).collect();
                let symbols = extract_symbol_hint(&segs);
                imports.push(RawImport {
                    specifier: spec,
                    kind: import_kind,
                    line,
                    symbols,
                });
            }
        }
    }
}

fn extract_symbol_hint(segs: &[String]) -> Vec<String> {
    let Some(last) = segs.last() else {
        return Vec::new();
    };
    if matches!(last.as_str(), "self" | "super" | "crate" | "*") || last.is_empty() {
        return Vec::new();
    }
    vec![last.clone()]
}

fn expand_use_tree(node: Node<'_>, source: &str, prefix: &[String], out: &mut Vec<String>) {
    match node.kind() {
        "identifier" | "crate" | "super" | "self" => {
            let seg = node_text(node, source).trim().to_string();
            let mut path = prefix.to_vec();
            if seg == "self" && !path.is_empty() {
                out.push(path.join("::"));
            } else {
                path.push(seg);
                out.push(path.join("::"));
            }
        }
        "scoped_identifier" => {
            let mut path = prefix.to_vec();
            collect_path_segments(node, source, &mut path);
            out.push(path.join("::"));
        }
        "use_as_clause" => {
            if let Some(path_node) = node.child_by_field_name("path") {
                expand_use_tree(path_node, source, prefix, out);
            }
        }
        "use_wildcard" => {
            let mut path = prefix.to_vec();
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_path_segments(child, source, &mut path);
            }
            if !path.is_empty() {
                path.push("*".to_string());
                out.push(path.join("::"));
            }
        }
        "scoped_use_list" => {
            let mut new_prefix = prefix.to_vec();
            if let Some(path_node) = node.child_by_field_name("path") {
                collect_path_segments(path_node, source, &mut new_prefix);
            }
            if let Some(list_node) = node.child_by_field_name("list") {
                expand_use_tree(list_node, source, &new_prefix, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                expand_use_tree(child, source, prefix, out);
            }
        }
        _ => {}
    }
}

fn collect_path_segments(node: Node<'_>, source: &str, out: &mut Vec<String>) {
    match node.kind() {
        "identifier" | "crate" | "super" | "self" | "type_identifier" => {
            let seg = node_text(node, source).trim();
            if !seg.is_empty() {
                out.push(seg.to_string());
            }
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            if let Some(path_node) = node.child_by_field_name("path") {
                collect_path_segments(path_node, source, out);
            }
            if let Some(name_node) = node.child_by_field_name("name") {
                collect_path_segments(name_node, source, out);
            }
        }
        _ => {}
    }
}

fn is_rust_path_candidate(first: &str) -> bool {
    if matches!(first, "crate" | "super" | "self") {
        return true;
    }
    if matches!(first, "std" | "core" | "alloc" | "Self") {
        return false;
    }
    // Lowercase-starting path segments are module or crate identifiers (not Types/Enums like `Option::Some`).
    first
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
}

fn is_cfg_test_attr(node: Node<'_>, source: &str) -> bool {
    let text = node_text(node, source);
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("cfg(test)")
}
