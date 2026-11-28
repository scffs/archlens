use std::collections::BTreeMap;
use std::fs;

use archlens::{AnalysisConfig, analyze};
use tempfile::TempDir;

fn file_view_clusters(sources: &[(&str, &str)]) -> BTreeMap<String, usize> {
    let temp = TempDir::new().expect("temporary repository");
    let root = temp.path();
    for &(file, source) in sources {
        let path = root.join(file);
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture folder");
        fs::write(path, source).expect("write source file");
    }

    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    let report = analyze(&config).expect("analyze fixture");
    let files = report
        .views
        .iter()
        .find(|view| view.id == "files")
        .expect("file view");
    files
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.louvain_cluster))
        .collect()
}

#[test]
fn file_view_ranks_connected_and_isolated_communities_by_size() {
    let clusters = file_view_clusters(&[
        ("assets/index.ts", "export const asset = '';\n"),
        ("ingest/index.ts", "import '../jobs/index';\n"),
        ("jobs/index.ts", "export const run = () => {};\n"),
        ("ledger/index.ts", "export const ledger = 0;\n"),
        (
            "transfers/index.ts",
            "import '../ledger/index';\nimport { credit, debit } from '../wallets/index';\n",
        ),
        ("wallets/index.ts", "export const credit = 1, debit = -1;\n"),
    ]);

    assert_eq!(
        clusters,
        BTreeMap::from([
            ("assets/index.ts".to_owned(), 2),
            ("ingest/index.ts".to_owned(), 1),
            ("jobs/index.ts".to_owned(), 1),
            ("ledger/index.ts".to_owned(), 0),
            ("transfers/index.ts".to_owned(), 0),
            ("wallets/index.ts".to_owned(), 0),
        ])
    );
}

#[test]
fn file_view_preserves_two_coupled_pairs_across_a_bridge() {
    let clusters = file_view_clusters(&[
        (
            "api.ts",
            "import { a, b, c, d } from './dashboard';\nimport './theme';\n",
        ),
        (
            "dashboard.ts",
            "import { a, b, c, d } from './ledger';\nexport const a = 1, b = 2, c = 3, d = 4;\n",
        ),
        ("ledger.ts", "export const a = 1, b = 2, c = 3, d = 4;\n"),
        ("theme.ts", "export const theme = 'light';\n"),
    ]);

    assert_eq!(clusters["api.ts"], clusters["theme.ts"]);
    assert_eq!(clusters["dashboard.ts"], clusters["ledger.ts"]);
    assert_ne!(clusters["api.ts"], clusters["dashboard.ts"]);
}

#[test]
fn file_view_joins_a_connected_community_after_reconsidering_its_members() {
    let clusters = file_view_clusters(&[
        (
            "api.ts",
            "import { report } from './billing';\nimport { account } from './customer';\nimport { a, b } from './ledger';\n",
        ),
        (
            "billing.ts",
            "import { a, b, c, d, e, f } from './ledger';\nexport const report = 1;\n",
        ),
        (
            "customer.ts",
            "import { a, b, c, d, e, f } from './ledger';\nexport const account = 1;\n",
        ),
        (
            "ledger.ts",
            "export const a = 1, b = 2, c = 3, d = 4, e = 5, f = 6;\n",
        ),
    ]);

    assert_eq!(clusters.len(), 4);
    assert!(clusters.values().all(|&id| id == 0));
}

#[test]
fn file_view_does_not_replace_a_cluster_with_a_weaker_candidate() {
    let clusters = file_view_clusters(&[
        (
            "api.ts",
            "import { a, b, c, d } from './ledger';\nimport './theme';\n",
        ),
        (
            "checkout.ts",
            "import { a, b, c, d } from './ledger';\nimport './receipt';\nimport './theme';\n",
        ),
        (
            "ledger.ts",
            "import './receipt';\nexport const a = 1, b = 2, c = 3, d = 4;\n",
        ),
        ("receipt.ts", "export const receipt = 1;\n"),
        ("theme.ts", "export const theme = 'light';\n"),
    ]);

    assert_eq!(clusters["api.ts"], clusters["theme.ts"]);
    assert_eq!(clusters["checkout.ts"], clusters["ledger.ts"]);
    assert_eq!(clusters["checkout.ts"], clusters["receipt.ts"]);
    assert_ne!(clusters["api.ts"], clusters["checkout.ts"]);
}
