use archlens::domain::algorithms::compute_propagation_and_nccd;

#[test]
fn propagation_includes_transitive_reachability_and_counts_each_node_once() {
    let edges = [
        (0, 1),
        (0, 2),
        (1, 3),
        (2, 3),
        (3, 1),
        (3, 4),
        (0, 1),
        (4, 4),
    ];

    // Reachability including self is [5, 3, 4, 3, 1, 1]. The balanced-tree reference is 14.
    assert_eq!(
        compute_propagation_and_nccd(6, &edges),
        (0.4722, 2.833, 1.214)
    );
}

#[test]
fn propagation_ignores_edges_with_unknown_endpoints() {
    let edges = [(0, 1), (3, 0), (0, 3)];

    assert_eq!(
        compute_propagation_and_nccd(3, &edges),
        (0.4444, 1.333, 0.8)
    );
}

#[test]
fn propagation_includes_self_in_small_and_edgeless_graphs() {
    assert_eq!(compute_propagation_and_nccd(0, &[]), (0.0, 0.0, 0.0));
    assert_eq!(compute_propagation_and_nccd(1, &[(0, 0)]), (1.0, 1.0, 1.0));
    assert_eq!(compute_propagation_and_nccd(3, &[]), (0.3333, 1.0, 0.6));
}
