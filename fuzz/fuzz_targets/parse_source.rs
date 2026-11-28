#![no_main]

use archlens::{Language, parser::parse_source};
use libfuzzer_sys::fuzz_target;

const SOURCES: [(&str, Language); 7] = [
    ("main.go", Language::Go),
    ("lib.rs", Language::Rust),
    ("main.js", Language::JavaScript),
    ("main.ts", Language::TypeScript),
    ("main.tsx", Language::TypeScript),
    ("main.py", Language::Python),
    ("App.vue", Language::Vue),
];

fuzz_target!(|input: &[u8]| {
    let Some((&selector, bytes)) = input.split_first() else {
        return;
    };
    let Ok(source) = std::str::from_utf8(bytes) else {
        return;
    };
    let (path, language) = SOURCES[usize::from(selector) % SOURCES.len()];
    let parsed = parse_source(path, source, language, selector & 0x80 != 0)
        .expect("initialize the selected source parser");

    assert_eq!(parsed.path, path);
    assert_eq!(parsed.language, language);
    assert_eq!(
        parsed.loc,
        source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
    );

    // Tree-sitter row numbers count LF bytes, including the last empty row.
    let line_count = source.bytes().filter(|&byte| byte == b'\n').count() + 1;
    let mut lines = parsed
        .raw_imports
        .iter()
        .map(|import| import.line)
        .chain(parsed.declarations.iter().map(|decl| decl.line));
    assert!(lines.all(|line| (1..=line_count).contains(&line)));
});
