use archlens::Language;
use archlens::domain::model::ImportKind;
use archlens::parser::parse_source;

#[test]
fn typescript_parser_preserves_import_kinds_and_original_named_symbols() {
    let source = "import makeClient, {
    connect as dial, close,
} from './client';
import type { Adapter as Port } from './contract';
import type{ Wire } from './wire';
import './register';
export { create as service } from './service';
import /* protocol */ type { Message } from './messages';
import type Model from './model';
";
    let parsed = parse_source("src/entry.ts", source, Language::TypeScript, false).unwrap();
    let imports: Vec<_> = parsed
        .raw_imports
        .iter()
        .map(|import| {
            (
                import.specifier.as_str(),
                import.kind,
                import.line,
                import
                    .symbols
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            )
        })
        .collect();

    assert_eq!(
        imports,
        [
            (
                "./client",
                ImportKind::Import,
                3,
                vec!["makeClient", "connect", "close"],
            ),
            ("./contract", ImportKind::TypeOnly, 4, vec!["Adapter"]),
            ("./wire", ImportKind::TypeOnly, 5, vec!["Wire"]),
            ("./register", ImportKind::Import, 6, vec![]),
            ("./service", ImportKind::ReExport, 7, vec!["create"]),
            ("./messages", ImportKind::TypeOnly, 8, vec!["Message"]),
            ("./model", ImportKind::TypeOnly, 9, vec!["Model"]),
        ]
    );
}

#[test]
fn type_is_a_value_binding_in_javascript_and_typescript_imports() {
    let source = "import * as type from './namespace';
import type from './default';
import { type as local } from './named';
";
    for (path, language) in [
        ("src/entry.js", Language::JavaScript),
        ("src/entry.ts", Language::TypeScript),
    ] {
        let parsed = parse_source(path, source, language, false).expect("parse value imports");
        let imports: Vec<_> = parsed
            .raw_imports
            .iter()
            .map(|import| {
                (
                    import.specifier.as_str(),
                    import.kind,
                    import
                        .symbols
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                )
            })
            .collect();

        assert_eq!(
            imports,
            [
                ("./namespace", ImportKind::Import, vec!["type"]),
                ("./default", ImportKind::Import, vec!["type"]),
                ("./named", ImportKind::Import, vec!["type"]),
            ],
            "{path}"
        );
    }
}
