use std::collections::BTreeSet;

use archlens::{FileEdge, FileGraph, FileNode, ImportKind, Language, SymbolCounts};

const MAX_FILES: u8 = 12;
const MAX_EDGES: usize = 48;

pub fn file_graph(input: &[u8]) -> FileGraph {
    let Some((&selector, input)) = input.split_first() else {
        return FileGraph::default();
    };
    let file_count = usize::from(selector % (MAX_FILES + 1)).min(input.len());
    if file_count == 0 {
        return FileGraph::default();
    }
    let (file_bytes, edge_bytes) = input.split_at(file_count);
    let files: Vec<_> = file_bytes
        .iter()
        .enumerate()
        .map(|(index, &byte)| {
            let (extension, language) = match byte % 3 {
                0 => ("rs", Language::Rust),
                1 => ("go", Language::Go),
                _ => ("ts", Language::TypeScript),
            };
            let folder = usize::from(byte / 3) % 3;
            FileNode {
                path: format!("src/group_{folder}/file_{index}.{extension}"),
                language,
                package: None,
                package_root: None,
                module_id: String::new(),
                loc: 1 + usize::from(byte),
                symbols: SymbolCounts::default(),
                declarations: Vec::new(),
                external_imports: Vec::new(),
                unresolved_imports: Vec::new(),
                git: None,
            }
        })
        .collect();

    let mut pairs = BTreeSet::new();
    let mut edges = Vec::new();
    for bytes in edge_bytes.chunks_exact(3).take(MAX_EDGES) {
        let from = usize::from(bytes[0]) % file_count;
        let to = usize::from(bytes[1]) % file_count;
        if from == to || !pairs.insert((from, to)) {
            continue;
        }
        let is_dip = bytes[2] % 2 != 0;
        edges.push(FileEdge {
            from: files[from].path.clone(),
            to: files[to].path.clone(),
            kind: if is_dip {
                ImportKind::TypeOnly
            } else {
                ImportKind::Import
            },
            line: 1,
            lines: vec![1],
            symbols: Vec::new(),
            is_dip,
        });
    }
    FileGraph { files, edges }
}
