use tree_sitter::Node;

use crate::domain::model::{
    ImportKind, Language, ParsedFile, RawImport, SymbolCounts, SymbolDecl, SymbolKind, Visibility,
};
use crate::error::Result;
use crate::parser::syntax::{LanguageParser, count_non_empty_lines, node_text, parse_tree, unquote};

pub struct GoParser;

impl LanguageParser for GoParser {
    fn parse(&self, rel_path: &str, source: &str, _include_tests: bool) -> Result<ParsedFile> {
        let mut raw_imports = Vec::new();
        let mut declarations = Vec::new();

        if let Some(tree) = parse_tree(tree_sitter_go::LANGUAGE.into(), "go", source)? {
            walk_go_root(tree.root_node(), source, &mut raw_imports, &mut declarations);
        }

        let symbols = SymbolCounts::from_declarations(&declarations);

        Ok(ParsedFile {
            path: rel_path.to_string(),
            language: Language::Go,
            loc: count_non_empty_lines(source),
            symbols,
            declarations,
            raw_imports,
            cfg_test_mods: Vec::new(),
            implemented_traits: Vec::new(),
        })
    }
}

fn walk_go_root(
    root: Node<'_>,
    source: &str,
    imports: &mut Vec<RawImport>,
    declarations: &mut Vec<SymbolDecl>,
) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "import_declaration" => collect_go_imports(child, source, imports),
            "type_declaration" => collect_go_types(child, source, declarations),
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(name_node, source).trim();
                    if !name.is_empty() {
                        declarations.push(SymbolDecl {
                            name: name.to_string(),
                            kind: SymbolKind::Function,
                            visibility: go_visibility(name),
                            is_abstract: false,
                            line: child.start_position().row + 1,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

fn go_visibility(name: &str) -> Visibility {
    if name.chars().next().is_some_and(char::is_uppercase) {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

fn collect_go_imports(node: Node<'_>, source: &str, imports: &mut Vec<RawImport>) {
    if node.kind() == "import_spec" {
        if let Some(path_node) = node.child_by_field_name("path") {
            let spec = unquote(node_text(path_node, source));
            if !spec.is_empty() {
                imports.push(RawImport {
                    specifier: spec,
                    kind: ImportKind::Import,
                    line: path_node.start_position().row + 1,
                    symbols: Vec::new(),
                });
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_go_imports(child, source, imports);
    }
}

fn collect_go_types(node: Node<'_>, source: &str, declarations: &mut Vec<SymbolDecl>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "type_spec" {
            let Some(name_node) = child.child_by_field_name("name") else {
                continue;
            };
            let name = node_text(name_node, source).trim();
            if name.is_empty() {
                continue;
            }
            let is_interface = child
                .child_by_field_name("type")
                .is_some_and(|t| t.kind() == "interface_type");
            declarations.push(SymbolDecl {
                name: name.to_string(),
                kind: if is_interface {
                    SymbolKind::Interface
                } else {
                    SymbolKind::Struct
                },
                visibility: go_visibility(name),
                is_abstract: is_interface,
                line: child.start_position().row + 1,
            });
        }
    }
}
