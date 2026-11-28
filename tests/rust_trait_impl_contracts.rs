use archlens::Language;
use archlens::domain::model::ImportKind;
use archlens::parser::parse_source;

#[test]
fn rust_parser_tracks_trait_implementation_names_and_internal_dependencies() {
    let source = "struct Gateway;
trait Local {}
impl Local for Gateway {}
impl vendor::External for Gateway {}
impl
    crate::api::Port<u32>
    for Gateway
{}
impl super::Read for Gateway {}
impl Gateway { fn new() -> Self { Self } }
";
    let parsed = parse_source("src/storage/gateway.rs", source, Language::Rust, false).unwrap();

    let traits: Vec<_> = parsed
        .implemented_traits
        .iter()
        .map(|(name, line)| (name.as_str(), *line))
        .collect();
    assert_eq!(
        traits,
        [("Local", 3), ("External", 4), ("Port", 6), ("Read", 9)]
    );

    let dependencies: Vec<_> = parsed
        .raw_imports
        .iter()
        .filter(|import| import.kind == ImportKind::TraitImpl)
        .map(|import| {
            (
                import.specifier.as_str(),
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
        dependencies,
        [
            ("crate::api::Port", 6, vec!["Port"]),
            ("super::Read", 9, vec!["Read"])
        ],
    );
}
