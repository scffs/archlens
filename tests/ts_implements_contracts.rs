use archlens::{Language, parser::parse_source};

#[test]
fn typescript_reports_implemented_interfaces_for_concrete_and_abstract_classes() {
    let source = r"interface Store<T> {}
interface Disposable {}
class Base {}
export class Repository
    extends Base
    implements Store<string>,
               Disposable {}
export abstract class Adapter implements Disposable {}
";
    let parsed = parse_source("repository.ts", source, Language::TypeScript, false).unwrap();

    assert_eq!(
        parsed.implemented_traits,
        [
            ("Store".to_owned(), 6),
            ("Disposable".to_owned(), 7),
            ("Disposable".to_owned(), 8),
        ],
    );
}

#[test]
fn vue_implements_entries_keep_component_source_lines() {
    let source = r"<template><div /></template>
<script lang='ts'>
interface Disposable {}
export class Service
    implements Disposable {}
</script>
";
    let parsed = parse_source("Page.vue", source, Language::Vue, false).unwrap();

    assert_eq!(parsed.implemented_traits, [("Disposable".to_owned(), 5)]);
}
