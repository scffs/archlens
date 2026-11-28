use std::collections::BTreeMap;
use std::fs;

use archlens::{AnalysisConfig, analyze};
use tempfile::TempDir;

#[test]
fn file_view_ranks_connected_and_isolated_communities_by_size() {
    let temp = TempDir::new().expect("temporary repository");
    let root = temp.path();
    for (folder, source) in [
        ("assets", "export const asset = '';\n"),
        ("ingest", "import '../jobs/index';\n"),
        ("jobs", "export const run = () => {};\n"),
        ("ledger", "export const ledger = 0;\n"),
        (
            "transfers",
            "import '../ledger/index';\nimport { credit, debit } from '../wallets/index';\n",
        ),
        ("wallets", "export const credit = 1, debit = -1;\n"),
    ] {
        let dir = root.join(folder);
        fs::create_dir(&dir).expect("create fixture folder");
        fs::write(dir.join("index.ts"), source).expect("write source file");
    }

    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let report = analyze(&config).expect("analyze fixture");
    let files = report
        .views
        .iter()
        .find(|view| view.id == "files")
        .expect("file view");
    let clusters: BTreeMap<_, _> = files
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.louvain_cluster))
        .collect();

    assert_eq!(
        clusters,
        BTreeMap::from([
            ("assets/index.ts", 2),
            ("ingest/index.ts", 1),
            ("jobs/index.ts", 1),
            ("ledger/index.ts", 0),
            ("transfers/index.ts", 0),
            ("wallets/index.ts", 0),
        ])
    );
}
