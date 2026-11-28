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
        ]
    );
}
