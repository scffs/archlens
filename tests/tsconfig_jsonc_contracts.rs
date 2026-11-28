use std::collections::BTreeMap;
use std::fs;

use archlens::resolver::{TsConfigInfo, js_ts::load_tsconfig};
use tempfile::TempDir;

fn tsconfig(text: &str) -> Option<TsConfigInfo> {
    let repo = TempDir::new().expect("temporary repository");
    let file = repo.path().join("tsconfig.json");
    fs::write(&file, text).expect("write tsconfig");
    load_tsconfig(repo.path(), &file, ".")
}

#[test]
fn tsconfig_accepts_jsonc_without_changing_quoted_aliases() {
    let config = tsconfig(
        r#"/* Leading comment on two lines.
   A bare * or / does not end it. */{
  "compilerOptions": {
    "baseUrl": "./src",// Paths are relative to this base.
    "paths": {/* Keep a comment next to a token. */
      "@models/*": ["types/*",],
      "registry://pkg/*slot*/,} \"quoted\" \\edge": ["registry/index.ts",],
    },
  },
}// final line comment"#,
    )
    .expect("read JSONC configuration");
    assert!(config.has_base_url);
    assert_eq!(config.base_dir, "src");
    let paths: BTreeMap<_, _> = config.paths.into_iter().collect();
    assert_eq!(
        paths,
        BTreeMap::from([
            ("@models/*".into(), vec!["src/types/*".into()]),
            (
                r#"registry://pkg/*slot*/,} "quoted" \edge"#.into(),
                vec!["src/registry/index.ts".into()],
            ),
        ])
    );
}

#[test]
fn tsconfig_accepts_a_line_comment_before_the_first_token() {
    let config = tsconfig(
        r#"// Shared TypeScript options
{"compilerOptions": {"baseUrl": "./src"}}"#,
    )
    .expect("read JSONC configuration");
    assert_eq!(config.base_dir, "src");
}

#[test]
fn tsconfig_ignores_an_incomplete_file_without_panicking() {
    for text in [
        r#"{"compilerOptions":{"paths":{"x": ["./file",   "#,
        r#"{"compilerOptions": { /* unfinished *"#,
        r#"{"compilerOptions":{"baseUrl":"."}}/"#,
    ] {
        let config = tsconfig(text);
        assert!(config.is_none(), "incomplete JSONC produced {config:?}");
    }
}
