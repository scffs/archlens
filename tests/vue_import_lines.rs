use archlens::Language;
use archlens::domain::model::ImportKind;
use archlens::parser::parse_source;

#[test]
fn vue_imports_keep_lines_relative_to_the_component_file() {
    let source = r#"<template><button>Open</button></template>

<script setup lang="ts">

import './register';
async function open() {
    return import('./panel');
}
</script>
"#;
    let parsed = parse_source("src/Panel.vue", source, Language::Vue, false).unwrap();
    let imports: Vec<_> = parsed
        .raw_imports
        .iter()
        .map(|import| (import.specifier.as_str(), import.kind, import.line))
        .collect();

    assert_eq!(
        imports,
        [
            ("./register", ImportKind::Import, 5),
            ("./panel", ImportKind::Dynamic, 7),
        ],
    );
}
