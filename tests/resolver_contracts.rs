use archlens::resolver::js_ts::resolve_js_ts_import;
use archlens::resolver::{ProjectIndex, ResolveOutcome};

#[test]
fn javascript_family_resolution_preserves_file_and_extension_precedence() {
    let mut index = ProjectIndex::default();
    index.known_files.extend(
        [
            "src/exact.js",
            "src/exact.ts",
            "src/mapped.ts",
            "src/mapped.tsx",
            "src/file.ts",
            "src/file.js",
            "src/file/index.ts",
            "src/directory/index.tsx",
            "src/directory/index.js",
            "src/vue/index.vue",
        ]
        .map(str::to_owned),
    );

    for (specifier, target) in [
        ("./exact.js", "src/exact.js"),
        ("./mapped.js", "src/mapped.ts"),
        ("./file", "src/file.ts"),
        ("./directory", "src/directory/index.tsx"),
        ("./vue", "src/vue/index.vue"),
    ] {
        assert_eq!(
            resolve_js_ts_import(&index, "src/entry.ts", specifier),
            ResolveOutcome::Internal(target.to_owned()),
            "{specifier}",
        );
    }
}
