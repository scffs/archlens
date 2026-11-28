use std::path::Path;

use tree_sitter::{Language as TsLanguage, Node};

use crate::domain::model::{
    ImportKind, Language, ParsedFile, RawImport, SymbolCounts, SymbolDecl, SymbolKind, Visibility,
};
use crate::error::Result;
use crate::parser::syntax::{
    LanguageParser, count_non_empty_lines, node_text, parse_tree, unquote,
};

#[derive(Debug, Clone, Copy)]
enum JsDialect {
    JavaScript,
    TypeScript,
    Tsx,
}

#[derive(Debug, Clone, Copy)]
pub struct JsTsParser {
    dialect: JsDialect,
}

struct JsWalkState {
    imports: Vec<RawImport>,
    declarations: Vec<SymbolDecl>,
    implemented_traits: Vec<(String, usize)>,
}

impl JsTsParser {
    pub const fn javascript() -> Self {
        Self {
            dialect: JsDialect::JavaScript,
        }
    }

    pub fn typescript(is_tsx: bool) -> Self {
        Self {
            dialect: if is_tsx {
                JsDialect::Tsx
            } else {
                JsDialect::TypeScript
            },
        }
    }

    fn ts_language(self) -> TsLanguage {
        match self.dialect {
            JsDialect::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            JsDialect::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            JsDialect::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }

    fn parse_script(self, source: &str, line_offset: usize, state: &mut JsWalkState) -> Result<()> {
        if let Some(tree) = parse_tree(&self.ts_language(), "js/ts", source)? {
            walk_program(tree.root_node(), source, line_offset, state);
        }
        Ok(())
    }
}

impl LanguageParser for JsTsParser {
    fn parse(&self, rel_path: &str, source: &str, _include_tests: bool) -> Result<ParsedFile> {
        let mut state = JsWalkState {
            imports: Vec::new(),
            declarations: Vec::new(),
            implemented_traits: Vec::new(),
        };

        self.parse_script(source, 0, &mut state)?;

        let language = match self.dialect {
            JsDialect::JavaScript => Language::JavaScript,
            JsDialect::TypeScript | JsDialect::Tsx => Language::TypeScript,
        };
        let symbols = SymbolCounts::from_declarations(&state.declarations);

        Ok(ParsedFile {
            path: rel_path.to_string(),
            language,
            loc: count_non_empty_lines(source),
            symbols,
            declarations: state.declarations,
            raw_imports: state.imports,
            cfg_test_mods: Vec::new(),
            implemented_traits: state.implemented_traits,
        })
    }
}

pub(super) struct TypeScriptParser;

impl LanguageParser for TypeScriptParser {
    fn parse(&self, rel_path: &str, source: &str, include_tests: bool) -> Result<ParsedFile> {
        let is_tsx = Path::new(rel_path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tsx"));
        JsTsParser::typescript(is_tsx).parse(rel_path, source, include_tests)
    }
}

/// Parser for Vue Single File Components (`.vue`), extracting all `<script>` / `<script setup>` blocks.
pub struct VueParser;

impl LanguageParser for VueParser {
    fn parse(&self, rel_path: &str, source: &str, _include_tests: bool) -> Result<ParsedFile> {
        let mut state = JsWalkState {
            imports: Vec::new(),
            declarations: Vec::new(),
            implemented_traits: Vec::new(),
        };

        let blocks = extract_vue_script_blocks(source);
        for block in &blocks {
            let parser = match block.lang.as_str() {
                "tsx" => JsTsParser::typescript(true),
                "ts" => JsTsParser::typescript(false),
                _ => JsTsParser::javascript(),
            };
            parser.parse_script(&block.content, block.start_line_offset, &mut state)?;
        }

        // A Vue SFC defines a concrete public UI component.
        if source.contains("<template") || !blocks.is_empty() {
            let comp_name = Path::new(rel_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Component")
                .to_string();
            state.declarations.push(SymbolDecl {
                name: comp_name,
                kind: SymbolKind::Component,
                visibility: Visibility::Public,
                is_abstract: false,
                line: 1,
            });
        }

        let symbols = SymbolCounts::from_declarations(&state.declarations);

        Ok(ParsedFile {
            path: rel_path.to_string(),
            language: Language::Vue,
            loc: count_non_empty_lines(source),
            symbols,
            declarations: state.declarations,
            raw_imports: state.imports,
            cfg_test_mods: Vec::new(),
            implemented_traits: state.implemented_traits,
        })
    }
}

struct ScriptBlock {
    lang: String,
    content: String,
    start_line_offset: usize,
}

fn extract_vue_script_blocks(source: &str) -> Vec<ScriptBlock> {
    let mut blocks = Vec::new();
    let lower = source.to_ascii_lowercase();
    let mut search_pos = 0;

    while let Some(rel_start) = lower[search_pos..].find("<script") {
        let tag_start = search_pos + rel_start;
        let after_tag_name = tag_start + "<script".len();
        if let Some(next_ch) = lower[after_tag_name..].chars().next()
            && !next_ch.is_whitespace()
            && next_ch != '>'
        {
            search_pos = after_tag_name;
            continue;
        }

        let Some(rel_tag_end) = lower[tag_start..].find('>') else {
            break;
        };
        let open_tag_end = tag_start + rel_tag_end;
        let open_tag = &lower[tag_start..=open_tag_end];

        let Some(rel_close) = lower[open_tag_end + 1..].find("</script") else {
            break;
        };
        let content_start = open_tag_end + 1;
        let content_end = content_start + rel_close;

        let lang = detect_script_lang(open_tag);
        let start_line_offset = source[..content_start]
            .bytes()
            .filter(|&b| b == b'\n')
            .count();
        let content = source[content_start..content_end].to_string();

        blocks.push(ScriptBlock {
            lang,
            content,
            start_line_offset,
        });

        search_pos = content_end + "</script".len();
    }

    blocks
}

fn detect_script_lang(open_tag_lower: &str) -> String {
    if open_tag_lower.contains("lang=\"tsx\"") || open_tag_lower.contains("lang='tsx'") {
        "tsx".to_string()
    } else if open_tag_lower.contains("lang=\"ts\"") || open_tag_lower.contains("lang='ts'") {
        "ts".to_string()
    } else {
        "js".to_string()
    }
}

fn walk_program(root: Node<'_>, source: &str, line_offset: usize, state: &mut JsWalkState) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        inspect_top_level(child, source, line_offset, state);
    }

    collect_dynamic_imports(root, source, line_offset, &mut state.imports);
}

fn inspect_top_level(node: Node<'_>, source: &str, line_offset: usize, state: &mut JsWalkState) {
    match node.kind() {
        "import_statement" => {
            if let Some(src) = node.child_by_field_name("source") {
                let mut cursor = node.walk();
                let is_type_only = node
                    .children(&mut cursor)
                    .any(|child| child.kind() == "type");
                let kind = if is_type_only {
                    ImportKind::TypeOnly
                } else {
                    ImportKind::Import
                };
                let symbols = collect_import_clause_symbols(node, source);
                push_import(src, source, kind, line_offset, symbols, &mut state.imports);
            }
        }
        "export_statement" => {
            if let Some(src) = node.child_by_field_name("source") {
                let symbols = collect_export_clause_symbols(node, source);
                push_import(
                    src,
                    source,
                    ImportKind::ReExport,
                    line_offset,
                    symbols,
                    &mut state.imports,
                );
            }
            if let Some(decl) = node.child_by_field_name("declaration") {
                classify_declaration(decl, source, line_offset, Visibility::Public, state);
            }
        }
        _ => classify_declaration(node, source, line_offset, Visibility::Private, state),
    }
}

fn collect_import_clause_symbols(import_node: Node<'_>, source: &str) -> Vec<String> {
    let mut symbols = Vec::new();
    let mut cursor = import_node.walk();
    for child in import_node.named_children(&mut cursor) {
        if child.kind() == "import_clause" {
            collect_specifier_names(child, source, &mut symbols);
        }
    }
    symbols
}

fn collect_export_clause_symbols(export_node: Node<'_>, source: &str) -> Vec<String> {
    let mut symbols = Vec::new();
    let mut cursor = export_node.walk();
    for child in export_node.named_children(&mut cursor) {
        if child.kind() == "export_clause" {
            collect_specifier_names(child, source, &mut symbols);
        }
    }
    symbols
}

fn collect_specifier_names(node: Node<'_>, source: &str, out: &mut Vec<String>) {
    match node.kind() {
        "import_specifier" | "export_specifier" => {
            if let Some(name) = node.child_by_field_name("name") {
                let s = node_text(name, source).trim();
                if !s.is_empty() {
                    out.push(s.to_string());
                }
            }
        }
        "identifier" => {
            let s = node_text(node, source).trim();
            if !s.is_empty() {
                out.push(s.to_string());
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_specifier_names(child, source, out);
            }
        }
    }
}

fn classify_declaration(
    node: Node<'_>,
    source: &str,
    line_offset: usize,
    visibility: Visibility,
    state: &mut JsWalkState,
) {
    let line = node.start_position().row + 1 + line_offset;
    match node.kind() {
        "interface_declaration" => {
            push_named_decl(
                node,
                source,
                SymbolKind::Interface,
                visibility,
                true,
                line,
                &mut state.declarations,
            );
        }
        "abstract_class_declaration" => {
            push_named_decl(
                node,
                source,
                SymbolKind::AbstractClass,
                visibility,
                true,
                line,
                &mut state.declarations,
            );
            collect_implements_clauses(node, source, line_offset, &mut state.implemented_traits);
        }
        "type_alias_declaration" => {
            if let Some(value) = node.child_by_field_name("value")
                && is_abstract_type_shape(value)
            {
                push_named_decl(
                    node,
                    source,
                    SymbolKind::TypeAlias,
                    visibility,
                    true,
                    line,
                    &mut state.declarations,
                );
            }
        }
        "class_declaration" => {
            push_named_decl(
                node,
                source,
                SymbolKind::Class,
                visibility,
                false,
                line,
                &mut state.declarations,
            );
            collect_implements_clauses(node, source, line_offset, &mut state.implemented_traits);
        }
        "enum_declaration" => {
            push_named_decl(
                node,
                source,
                SymbolKind::Enum,
                visibility,
                false,
                line,
                &mut state.declarations,
            );
        }
        "function_declaration" | "generator_function_declaration" => {
            push_named_decl(
                node,
                source,
                SymbolKind::Function,
                visibility,
                false,
                line,
                &mut state.declarations,
            );
        }
        "lexical_declaration" | "variable_declaration" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "variable_declarator"
                    && let Some(value) = child.child_by_field_name("value")
                    && matches!(
                        value.kind(),
                        "arrow_function" | "function_expression" | "class"
                    )
                {
                    let name = child
                        .child_by_field_name("name")
                        .map_or("anonymous", |n| node_text(n, source).trim());
                    let kind = if value.kind() == "class" {
                        SymbolKind::Class
                    } else {
                        SymbolKind::Function
                    };
                    state.declarations.push(SymbolDecl {
                        name: name.to_string(),
                        kind,
                        visibility,
                        is_abstract: false,
                        line: child.start_position().row + 1 + line_offset,
                    });
                }
            }
        }
        "ambient_declaration" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                classify_declaration(child, source, line_offset, Visibility::Public, state);
            }
        }
        _ => {}
    }
}

fn push_named_decl(
    node: Node<'_>,
    source: &str,
    kind: SymbolKind,
    visibility: Visibility,
    is_abstract: bool,
    line: usize,
    declarations: &mut Vec<SymbolDecl>,
) {
    let name = node
        .child_by_field_name("name")
        .map_or("default", |n| node_text(n, source).trim());
    if !name.is_empty() {
        declarations.push(SymbolDecl {
            name: name.to_string(),
            kind,
            visibility,
            is_abstract,
            line,
        });
    }
}

fn collect_implements_clauses(
    class_node: Node<'_>,
    source: &str,
    line_offset: usize,
    out: &mut Vec<(String, usize)>,
) {
    let mut cursor = class_node.walk();
    for child in class_node.named_children(&mut cursor) {
        if child.kind() == "class_heritage" {
            let mut hc = child.walk();
            for clause in child.named_children(&mut hc) {
                if clause.kind() == "implements_clause" {
                    let mut ic = clause.walk();
                    for item in clause.named_children(&mut ic) {
                        let raw = node_text(item, source);
                        let base = raw.split('<').next().unwrap_or(raw).trim();
                        if !base.is_empty() {
                            out.push((
                                base.to_string(),
                                item.start_position().row + 1 + line_offset,
                            ));
                        }
                    }
                }
            }
        }
    }
}

fn is_abstract_type_shape(node: Node<'_>) -> bool {
    match node.kind() {
        "object_type" | "function_type" => true,
        "parenthesized_type" => node.named_child(0).is_some_and(is_abstract_type_shape),
        "intersection_type" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).any(is_abstract_type_shape)
        }
        _ => false,
    }
}

fn collect_dynamic_imports(
    node: Node<'_>,
    source: &str,
    line_offset: usize,
    imports: &mut Vec<RawImport>,
) {
    if node.kind() == "call_expression"
        && let Some(func) = node.child_by_field_name("function")
    {
        let is_dynamic = func.kind() == "import"
            || (func.kind() == "identifier" && node_text(func, source) == "require");
        if is_dynamic
            && let Some(args) = node.child_by_field_name("arguments")
            && let Some(first_arg) = args.named_child(0)
            && (first_arg.kind() == "string" || first_arg.kind() == "template_string")
        {
            let spec = unquote(node_text(first_arg, source));
            if !spec.is_empty() && !spec.contains("${") {
                imports.push(RawImport {
                    specifier: spec,
                    kind: ImportKind::Dynamic,
                    line: node.start_position().row + 1 + line_offset,
                    symbols: Vec::new(),
                });
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.child_count() > 0 || child.kind() == "call_expression" {
            collect_dynamic_imports(child, source, line_offset, imports);
        }
    }
}

fn push_import(
    src_node: Node<'_>,
    source: &str,
    kind: ImportKind,
    line_offset: usize,
    symbols: Vec<String>,
    imports: &mut Vec<RawImport>,
) {
    let specifier = unquote(node_text(src_node, source));
    if !specifier.is_empty() {
        imports.push(RawImport {
            specifier,
            kind,
            line: src_node.start_position().row + 1 + line_offset,
            symbols,
        });
    }
}
