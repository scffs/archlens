use std::fs;

use archlens::config::{ArchitectureRule, RuleKind};
use archlens::{AnalysisConfig, analyze};
use tempfile::TempDir;

#[test]
fn projected_dependencies_keep_counts_and_distinguish_type_only_edges() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    let files = [
        (
            "client/entry.ts",
            "import { helper } from './helper';\n\
             import type { Contract } from '../service/contracts';\n\
             import { runtime } from '../service/runtime';\n\
             export const entry: Contract = helper(runtime);",
        ),
        (
            "client/helper.ts",
            "import { runtime } from '../service/runtime';\nexport const helper = () => runtime;",
        ),
        (
            "service/contracts.ts",
            "import type { Transit } from '../relay/transit';\nexport type Contract = Transit;",
        ),
        (
            "service/runtime.ts",
            "import type { Transit } from '../relay/transit';\n\
             export const runtime: Transit = { value: 1 };",
        ),
        (
            "relay/transit.ts",
            "import { runtime } from '../service/runtime';\n\
             import { entry } from '../client/entry';\n\
             export interface Transit { value: number }\nexport const transit = [runtime, entry];",
        ),
        ("unused/idle.ts", "export const idle = 0;"),
    ];
    for (path, source) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("fixture directory")).expect("create directory");
        fs::write(path, source).expect("write fixture");
    }

    let mut config = AnalysisConfig::new(root);
    config.git.enabled = false;
    config.sdp_tolerance = 0.1;
    config.rules = vec![ArchitectureRule {
        name: "client boundary".into(),
        kind: RuleKind::Forbid,
        from: vec!["client/entry.ts".into()],
        to: vec!["service/**".into()],
        allow_dip: false,
        reason: None,
    }];

    let report = analyze(&config).expect("analyze dependencies");
    let view = report.view("folders-d1").expect("first-level folder view");
    let node = |id| view.nodes.iter().find(|node| node.id == id).expect("node");
    let client = node("client");
    let service = node("service");
    assert_eq!((client.afferent_coupling, client.efferent_coupling), (1, 2));
    assert_eq!((client.component_fan_in, client.component_fan_out), (1, 1));
    assert_eq!(service.afferent_coupling, 3);
    assert_eq!(service.efferent_coupling, 1);
    assert_eq!(client.internal_edges, 1);
    assert_eq!(client.cohesion, 1.0);
    assert_eq!(client.dip_ratio, 0.25);
    assert_eq!(service.dip_ratio, 0.5);
    assert_eq!(node("unused").dip_ratio, 0.0);

    let client_edge = view
        .edges
        .iter()
        .find(|edge| edge.from == "client" && edge.to == "service")
        .expect("client to service edge");
    assert_eq!((client_edge.weight, client_edge.dip_weight), (3, 1));
    assert!(client_edge.is_rule_violation);
    assert_eq!(report.rule_violations.len(), 2);
    assert_eq!(view.summary.rule_violation_count, 1);

    let violations: Vec<_> = view
        .sdp_violations
        .iter()
        .map(|violation| {
            (
                violation.from.as_str(),
                violation.to.as_str(),
                violation.delta,
                violation.is_dip_only,
            )
        })
        .collect();
    assert_eq!(
        violations,
        [
            ("relay", "client", 0.1667, false),
            ("service", "relay", 0.25, true)
        ]
    );

    config.sdp_tolerance = 0.25;
    let report = analyze(&config).expect("analyze at the tolerance boundary");
    assert!(
        report
            .view("folders-d1")
            .expect("folder view")
            .sdp_violations
            .is_empty()
    );
}
