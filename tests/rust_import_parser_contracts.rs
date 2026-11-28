use archlens::parser::parse_source;
use archlens::{ImportKind, Language};

#[test]
fn rust_parser_preserves_nested_use_symbols_and_qualified_body_references() {
    let source = r"pub use crate::api::{self, Original as Alias, nested::*};
use super::local::Type;
impl Runner {
    fn execute(input: crate::model::Input) {
        use crate::helpers::configure;
        if ready() {
            _hooks::run();
            adapters::apply();
            Option::Some(input);
            std::mem::drop(0);
            Self::from(0);
        }
    }
}
";
    let parsed = parse_source("src/runner.rs", source, Language::Rust, false).expect("parse Rust");
    let imports: Vec<_> = parsed
        .raw_imports
        .iter()
        .map(|entry| {
            (
                entry.specifier.as_str(),
                entry.kind,
                entry.line,
                entry.symbols.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        })
        .collect();

    assert_eq!(
        imports,
        [
            ("crate::api", ImportKind::ReExport, 1, vec!["api"]),
            (
                "crate::api::Original",
                ImportKind::ReExport,
                1,
                vec!["Original"]
            ),
            ("crate::api::nested::*", ImportKind::ReExport, 1, vec![]),
            ("super::local::Type", ImportKind::Import, 2, vec!["Type"]),
            ("crate::model::Input", ImportKind::Import, 4, vec!["Input"]),
            (
                "crate::helpers::configure",
                ImportKind::Import,
                5,
                vec!["configure"]
            ),
            ("_hooks::run", ImportKind::Import, 7, vec!["run"]),
            ("adapters::apply", ImportKind::Import, 8, vec!["apply"]),
        ]
    );
}
