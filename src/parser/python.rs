use tree_sitter::{Node, Parser};

use crate::domain::model::{
    ImportKind, Language, ParsedFile, RawImport, SymbolCounts, SymbolDecl, SymbolKind, Visibility,
};
use crate::error::{ArchLensError, Result};
use crate::parser::{LanguageParser, count_non_empty_lines, node_text};

pub struct PythonParser;

impl LanguageParser for PythonParser {
    fn parse(&self, rel_path: &str, source: &str, _include_tests: bool) -> Result<ParsedFile> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .map_err(|err| ArchLensError::ParserInit {
                language: "python",
                message: err.to_string(),
            })?;

        let mut raw_imports = Vec::new();
        let mut declarations = Vec::new();

        if let Some(tree) = parser.parse(source, None) {
            let root = tree.root_node();
            collect_top_level_symbols(root, source, &mut declarations);
            collect_all_imports(root, source, &mut raw_imports);
        }

        let symbols = SymbolCounts::from_declarations(&declarations);

        Ok(ParsedFile {
            path: rel_path.to_string(),
            language: Language::Python,
            loc: count_non_empty_lines(source),
            symbols,
            declarations,
            raw_imports,
            cfg_test_mods: Vec::new(),
            implemented_traits: Vec::new(),
        })
    }
}

fn collect_top_level_symbols(root: Node<'_>, source: &str, declarations: &mut Vec<SymbolDecl>) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let target = if child.kind() == "decorated_definition" {
            child.child_by_field_name("definition").unwrap_or(child)
        } else {
            child
        };

        let line = target.start_position().row + 1;
        match target.kind() {
            "class_definition" => {
                let name = target
                    .child_by_field_name("name")
                    .map_or("", |n| node_text(n, source).trim());
                if name.is_empty() {
                    continue;
                }
                let is_abs = is_abstract_python_class(target, source);
                declarations.push(SymbolDecl {
                    name: name.to_string(),
                    kind: if is_abs {
                        SymbolKind::Protocol
                    } else {
                        SymbolKind::Class
                    },
                    visibility: py_visibility(name),
                    is_abstract: is_abs,
                    line,
                });
            }
            "function_definition" => {
                let name = target
                    .child_by_field_name("name")
                    .map_or("", |n| node_text(n, source).trim());
                if name.is_empty() {
                    continue;
                }
                declarations.push(SymbolDecl {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    visibility: py_visibility(name),
                    is_abstract: false,
                    line,
                });
            }
            _ => {}
        }
    }
}

fn py_visibility(name: &str) -> Visibility {
    if name.starts_with('_') && !name.starts_with("__") {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

fn is_abstract_python_class(class_node: Node<'_>, source: &str) -> bool {
    if let Some(supers) = class_node.child_by_field_name("superclasses") {
        let text = node_text(supers, source);
        if text.contains("ABC") || text.contains("Protocol") || text.contains("ABCMeta") {
            return true;
        }
    }

    if let Some(body) = class_node.child_by_field_name("body") {
        let body_text = node_text(body, source);
        if body_text.contains("@abstractmethod") || body_text.contains("@abc.abstractmethod") {
            return true;
        }
    }

    false
}

fn collect_all_imports(node: Node<'_>, source: &str, imports: &mut Vec<RawImport>) {
    match node.kind() {
        "import_statement" => {
            let line = node.start_position().row + 1;
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "dotted_name" => {
                        let name = node_text(child, source).trim();
                        if !name.is_empty() {
                            imports.push(RawImport {
                                specifier: name.to_string(),
                                kind: ImportKind::Import,
                                line,
                                symbols: Vec::new(),
                            });
                        }
                    }
                    "aliased_import" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = node_text(name_node, source).trim();
                            if !name.is_empty() {
                                imports.push(RawImport {
                                    specifier: name.to_string(),
                                    kind: ImportKind::Import,
                                    line,
                                    symbols: Vec::new(),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        "import_from_statement" => {
            let line = node.start_position().row + 1;
            let module = node
                .child_by_field_name("module_name")
                .map_or_default(|n| node_text(n, source).trim().to_string());

            let mut imported_names = Vec::new();
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if Some(child.id()) == node.child_by_field_name("module_name").map(|n| n.id()) {
                    continue;
                }
                match child.kind() {
                    "dotted_name" | "identifier" => {
                        let name = node_text(child, source).trim();
                        if !name.is_empty() {
                            imported_names.push(name.to_string());
                        }
                    }
                    "aliased_import" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = node_text(name_node, source).trim();
                            if !name.is_empty() {
                                imported_names.push(name.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }

            if imported_names.is_empty() {
                if !module.is_empty() {
                    imports.push(RawImport {
                        specifier: module,
                        kind: ImportKind::Import,
                        line,
                        symbols: Vec::new(),
                    });
                }
            } else {
                for name in imported_names {
                    let specifier = if module.is_empty() {
                        name.clone()
                    } else {
                        format!("{module}::{name}")
                    };
                    imports.push(RawImport {
                        specifier,
                        kind: ImportKind::Import,
                        line,
                        symbols: vec![name],
                    });
                }
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.child_count() > 0 {
                    collect_all_imports(child, source, imports);
                }
            }
        }
    }
}
