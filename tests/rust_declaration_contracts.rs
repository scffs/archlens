use archlens::Language;
use archlens::domain::model::{SymbolKind, Visibility};
use archlens::parser::parse_source;

#[test]
fn rust_parser_distinguishes_concrete_declarations_and_abstract_aliases() {
    let source = "pub enum Encoding { Compact, Verbose }
pub(crate) union Payload { number: u32, bytes: [u8; 4] }

pub type Writer = dyn std::io::Write;
pub(crate) type Serialize = fn(&Payload) -> usize;
type Index = usize;
";
    let parsed = parse_source("src/codec.rs", source, Language::Rust, false).unwrap();
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
            ("Encoding", SymbolKind::Enum, Visibility::Public, false, 1),
            ("Payload", SymbolKind::Enum, Visibility::Internal, false, 2),
            ("Writer", SymbolKind::TypeAlias, Visibility::Public, true, 4),
            (
                "Serialize",
                SymbolKind::TypeAlias,
                Visibility::Internal,
                true,
                5,
            ),
        ],
    );
}
