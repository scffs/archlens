use std::path::Path;

use archlens::Language;
use archlens::domain::model::{ImportKind, SymbolKind, Visibility};
use archlens::parser::parse_source;
use archlens::resolver::{join_posix, normalize_posix, parent_dir, path_is_under};
use archlens::scanner::{build_glob_set, normalize_rel_path};

#[test]
fn repository_paths_distinguish_segments_and_normalize_components() {
    assert!(path_is_under("pkg", "pkg"));
    assert!(path_is_under("pkg/src/item.go", "pkg"));
    assert!(path_is_under("any/path", ""));
    assert!(!path_is_under("pkg-secondary/item.go", "pkg"));
    assert!(!path_is_under("other/item.go", "pkg"));

    assert_eq!(
        normalize_rel_path(Path::new("repo"), Path::new("repo/pkg/a.go")),
        "pkg/a.go"
    );
    assert_eq!(parent_dir("pkg/src/a.go"), "pkg/src");
    assert_eq!(parent_dir("a.go"), "");
    assert_eq!(
        join_posix("pkg/src", "../other/./item.go"),
        "pkg/other/item.go"
    );
    assert_eq!(join_posix("", "pkg/item.go"), "pkg/item.go");
    assert_eq!(join_posix("pkg/item.go", ""), "pkg/item.go");
    assert_eq!(
        normalize_posix("../../pkg//./src/../item.go"),
        "pkg/item.go"
    );

    let excludes = build_glob_set(&["vendor/**".into(), "**/*_test.go".into()]).unwrap();
    assert!(excludes.is_match("vendor/nested/item.go"));
    assert!(excludes.is_match("pkg/item_test.go"));
    assert!(!excludes.is_match("vendored/item.go"));
    assert!(build_glob_set(&["[".into()]).is_err());
}

#[test]
fn go_parser_preserves_imports_declarations_and_one_based_lines() {
    let source = concat!(
        "package core\n\n",
        "import (\n",
        "    \"fmt\"\n",
        "    _ `net/http`\n",
        "    \"\"\n",
        ")\n\n",
        "type Port interface{ Run() }\n",
        "type internal struct{}\n",
        "func Exported() {}\n",
        "func hidden() {}\n",
    );
    let parsed = parse_source("pkg/core.go", source, Language::Go, false).unwrap();
    let imports: Vec<_> = parsed
        .raw_imports
        .iter()
        .map(|import| (import.specifier.as_str(), import.kind, import.line))
        .collect();
    assert_eq!(
        imports,
        [
            ("fmt", ImportKind::Import, 4),
            ("net/http", ImportKind::Import, 5)
        ]
    );

    let declarations: Vec<_> = parsed
        .declarations
        .iter()
        .map(|decl| {
            (
                decl.name.as_str(),
                decl.kind,
                decl.visibility,
                decl.is_abstract,
                decl.line,
            )
        })
        .collect();
    assert_eq!(
        declarations,
        [
            ("Port", SymbolKind::Interface, Visibility::Public, true, 9),
            (
                "internal",
                SymbolKind::Struct,
                Visibility::Private,
                false,
                10
            ),
            (
                "Exported",
                SymbolKind::Function,
                Visibility::Public,
                false,
                11
            ),
            (
                "hidden",
                SymbolKind::Function,
                Visibility::Private,
                false,
                12
            ),
        ]
    );
}
