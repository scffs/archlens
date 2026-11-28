use archlens::resolver::js_ts::resolve_js_ts_import;
use archlens::resolver::{JsPackageInfo, ProjectIndex, ResolveOutcome};

#[test]
fn javascript_resolves_relative_directory_imports_and_preserves_remote_urls() {
    let index = ProjectIndex {
        known_files: ["workspace/src/index.ts", "workspace/index.ts"]
            .map(str::to_owned)
            .into(),
        ..ProjectIndex::default()
    };

    for (specifier, target) in [
        (".", "workspace/src/index.ts"),
        ("..", "workspace/index.ts"),
    ] {
        assert_eq!(
            resolve_js_ts_import(&index, "workspace/src/entry.ts", specifier),
            ResolveOutcome::Internal(target.to_owned()),
            "{specifier}",
        );
    }

    for url in [
        "http://example.test/module.ts",
        "https://example.test/module.ts",
    ] {
        assert_eq!(
            resolve_js_ts_import(&index, "workspace/src/entry.ts", url),
            ResolveOutcome::External(url.to_owned()),
            "{url}",
        );
    }
}

#[test]
fn javascript_resolves_workspace_scoped_packages_by_root_or_subpath() {
    let index = ProjectIndex {
        known_files: ["packages/core/entry.ts", "packages/core/src/feature.ts"]
            .map(str::to_owned)
            .into(),
        js_packages: [(
            "@scope/core".to_owned(),
            JsPackageInfo {
                name: "@scope/core".to_owned(),
                dir: "packages/core".to_owned(),
                entry_candidates: vec!["entry.ts".to_owned()],
            },
        )]
        .into(),
        ..ProjectIndex::default()
    };

    for (specifier, target) in [
        ("@scope/core", "packages/core/entry.ts"),
        ("@scope/core/feature", "packages/core/src/feature.ts"),
    ] {
        assert_eq!(
            resolve_js_ts_import(&index, "app/entry.ts", specifier),
            ResolveOutcome::Internal(target.to_owned()),
            "{specifier}",
        );
    }
    assert_eq!(
        resolve_js_ts_import(&index, "app/entry.ts", "@scope/remote/feature"),
        ResolveOutcome::External("@scope/remote".to_owned()),
    );
}
