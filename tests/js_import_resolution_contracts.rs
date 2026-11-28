use archlens::resolver::js_ts::{
    AliasFallback, resolve_js_ts_import, resolve_js_ts_import_with_aliases,
};
use archlens::resolver::{JsPackageInfo, ProjectIndex, ResolveOutcome, TsConfigInfo};

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

#[test]
fn tsconfig_paths_match_complete_patterns_before_the_alias_fallback() {
    let index = ProjectIndex {
        known_files: [
            "fixtures/client/index.ts",
            "fixtures/entry.ts",
            "fixtures/fallback.ts",
        ]
        .map(str::to_owned)
        .into(),
        tsconfigs: vec![TsConfigInfo {
            dir: "app".to_owned(),
            base_dir: "app".to_owned(),
            paths: vec![
                (
                    "@app/*/test".to_owned(),
                    vec!["fixtures/*/index".to_owned()],
                ),
                ("@entry".to_owned(), vec!["fixtures/entry.ts".to_owned()]),
            ],
            has_base_url: false,
        }],
        ..ProjectIndex::default()
    };

    assert_eq!(
        resolve_js_ts_import(&index, "app/main.ts", "@app/client/test"),
        ResolveOutcome::Internal("fixtures/client/index.ts".to_owned()),
    );

    // The prefix and suffix overlap on the slash in "@app/test".
    for (specifier, package) in [
        ("@app/client/prod", "@app/client"),
        ("@dev/client/test", "@dev/client"),
        ("@app/test", "@app/test"),
    ] {
        assert_eq!(
            resolve_js_ts_import(&index, "app/main.ts", specifier),
            ResolveOutcome::External(package.to_owned()),
            "{specifier}",
        );
    }

    assert_eq!(
        resolve_js_ts_import_with_aliases(&index, "app/main.ts", "@entry", |_, _, _| {
            Some(AliasFallback::Candidates(vec![
                "fixtures/fallback.ts".to_owned(),
            ]))
        }),
        ResolveOutcome::Internal("fixtures/entry.ts".to_owned()),
    );
}

#[test]
fn tsconfig_base_url_requires_a_configured_base_and_a_public_import() {
    let index = ProjectIndex {
        known_files: [
            "app/lib/helper.ts",
            "app/lib/#runtime.ts",
            "app/generated/runtime.ts",
            "library/helper.ts",
            "library/configured.ts",
        ]
        .map(str::to_owned)
        .into(),
        tsconfigs: vec![
            TsConfigInfo {
                dir: "app".to_owned(),
                base_dir: "app/lib".to_owned(),
                paths: vec![],
                has_base_url: true,
            },
            TsConfigInfo {
                dir: "library".to_owned(),
                base_dir: "library".to_owned(),
                paths: vec![(
                    "@configured".to_owned(),
                    vec!["library/configured.ts".to_owned()],
                )],
                has_base_url: false,
            },
        ],
        ..ProjectIndex::default()
    };

    assert_eq!(
        resolve_js_ts_import(&index, "app/main.ts", "helper"),
        ResolveOutcome::Internal("app/lib/helper.ts".to_owned()),
    );
    assert_eq!(
        resolve_js_ts_import(&index, "library/main.ts", "helper"),
        ResolveOutcome::External("helper".to_owned()),
    );
    assert_eq!(
        resolve_js_ts_import_with_aliases(
            &index,
            "app/main.ts",
            "#runtime?worker",
            |_, _, alias| {
                assert_eq!(alias, "#runtime");
                Some(AliasFallback::Candidates(vec![
                    "app/generated/runtime.ts".to_owned(),
                ]))
            }
        ),
        ResolveOutcome::Internal("app/generated/runtime.ts".to_owned()),
    );
}
