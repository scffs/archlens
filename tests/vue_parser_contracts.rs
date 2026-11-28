use archlens::Language;
use archlens::domain::model::{ImportKind, SymbolDecl, SymbolKind, Visibility};
use archlens::parser::parse_source;

#[test]
fn vue_parses_adjacent_scripts_with_original_file_line_numbers() {
    let source = "\
<template>
  <script-preview />
</template>
<script>
import { normal } from './normal'</script><script setup>
import { setup } from './setup'
</script>
";
    let parsed =
        parse_source("src/Panel.vue", source, Language::Vue, false).expect("parse Vue SFC");
    let imports: Vec<_> = parsed
        .raw_imports
        .iter()
        .map(|import| (import.specifier.as_str(), import.kind, import.line))
        .collect();

    assert_eq!(
        imports,
        [
            ("./normal", ImportKind::Import, 5),
            ("./setup", ImportKind::Import, 6),
        ]
    );
}

#[test]
fn vue_uses_the_script_lang_for_typescript_and_tsx() {
    for (attribute, body) in [
        ("lang='ts'", "export interface Props { title: string }"),
        (
            "lang=\"tsx\"",
            "export interface Props { title: string }\nexport function render() { return <section />; }",
        ),
    ] {
        let source = format!("<script setup {attribute}>\n{body}\n</script>");
        let parsed =
            parse_source("src/Panel.vue", &source, Language::Vue, false).expect("parse Vue SFC");

        assert!(parsed.declarations.contains(&SymbolDecl {
            name: "Props".into(),
            kind: SymbolKind::Interface,
            visibility: Visibility::Public,
            is_abstract: true,
            line: 2,
        }));
    }
}

#[test]
fn vue_reports_a_component_for_template_or_script() {
    for (source, is_component) in [
        ("<template><main /></template>", true),
        ("<script></script>", true),
        ("<style scoped>main { color: black }</style>", false),
    ] {
        let parsed =
            parse_source("src/Panel.vue", source, Language::Vue, false).expect("parse Vue SFC");
        let component = SymbolDecl {
            name: "Panel".into(),
            kind: SymbolKind::Component,
            visibility: Visibility::Public,
            is_abstract: false,
            line: 1,
        };

        assert_eq!(parsed.declarations.contains(&component), is_component);
    }
}
