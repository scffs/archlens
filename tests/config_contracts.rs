use std::fs;

use archlens::AnalysisConfig;
use tempfile::TempDir;

#[test]
fn empty_toml_matches_a_no_config_run() {
    let temp = TempDir::new().expect("tempdir");
    let without_file = AnalysisConfig::load_or_default(temp.path()).expect("default config");
    fs::write(temp.path().join("archlens.toml"), "").expect("write empty config");
    let with_empty_file = AnalysisConfig::load_or_default(temp.path()).expect("read empty config");

    assert_eq!(with_empty_file.root, without_file.root);
    assert_eq!(
        serde_json::to_value(&with_empty_file).expect("serialize file config"),
        serde_json::to_value(&without_file).expect("serialize default config")
    );
}

#[test]
fn partial_toml_overrides_only_present_values() {
    let temp = TempDir::new().expect("tempdir");
    fs::write(
        temp.path().join("archlens.toml"),
        "main_sequence_band = 0.0\nexclude = []\n[git]\nenabled = false\n",
    )
    .expect("write partial config");
    let actual = AnalysisConfig::load_or_default(temp.path()).expect("load partial config");
    let mut expected = AnalysisConfig::new(temp.path());
    expected.main_sequence_band = 0.0;
    expected.exclude.clear();
    expected.git.enabled = false;
    assert_eq!(
        serde_json::to_value(&actual).expect("serialize file config"),
        serde_json::to_value(&expected).expect("serialize overridden config")
    );
}
