use archlens::parser::parse_source;
use archlens::{ImportKind, Language};

#[test]
fn python_keeps_nested_imports_original_names_and_lines() {
    let source = "\
# Dependencies for the loader
import package.core, package.extra as extra
from .defaults import *

def load():
    from .models import Book, Sale as Purchase
    import json as codec
    return Book, Purchase, codec
";
    let parsed =
        parse_source("package/loader.py", source, Language::Python, false).expect("parse Python");
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
            ("package.core", ImportKind::Import, 2, vec![]),
            ("package.extra", ImportKind::Import, 2, vec![]),
            (".defaults", ImportKind::Import, 3, vec![]),
            (".models::Book", ImportKind::Import, 6, vec!["Book"]),
            (".models::Sale", ImportKind::Import, 6, vec!["Sale"]),
            ("json", ImportKind::Import, 7, vec![]),
        ]
    );
}
