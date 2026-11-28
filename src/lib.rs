//! `archlens` — language-agnostic Clean Architecture coupling, Main Sequence (A/I),
//! Propagation Cost / NCCD, cycle-breaker, Louvain clustering, Git hotspots & temporal coupling,
//! declarative architecture rules, and interactive compound graph / DSM analyzer.

pub mod checks;
pub mod config;
pub mod diff;
pub mod domain;
pub mod error;
pub mod git;
pub mod parser;
pub mod plugins;
pub mod report;
pub mod resolver;
pub mod scanner;

mod language;
mod paths;

use std::collections::BTreeMap;

pub use config::{
    AnalysisConfig, ArchitectureRule, ComponentRule, GitConfig, LayerRule, RuleKind,
    ThresholdConfig,
};
pub use domain::metrics::{
    AnalysisReport, ArchitectureView, ArchitectureZone, BaselineDiff, CheckViolation,
    CycleBreakEdge, CycleGroup, DetailedFileEdge, MisplacedFile, NodeMetrics, RuleViolation,
    SdpViolation, ViewEdge, ViewSummary,
};
pub use domain::model::{
    FileEdge, FileGitMetrics, FileGraph, FileNode, GitAnalysis, ImportKind, Language, SymbolCounts,
    SymbolDecl, SymbolKind, TemporalCoupling, Visibility,
};
pub use error::{ArchLensError, Result};
pub use plugins::AnalysisPlugins;

/// Scan, parse, resolve, enrich with Git history, and compute all architecture views & metrics.
pub fn analyze(config: &AnalysisConfig) -> Result<AnalysisReport> {
    analyze_with_plugins(config, &AnalysisPlugins::default())
}

/// Analyze a repository with extensions supplied explicitly by the caller.
pub fn analyze_with_plugins(
    config: &AnalysisConfig,
    plugins: &AnalysisPlugins<'_>,
) -> Result<AnalysisReport> {
    let canonical_root = config
        .root
        .canonicalize()
        .unwrap_or_else(|_| config.root.clone());

    let scanned =
        scanner::scan_repository_with(config, |ext| plugins.language_for_extension(ext))?;
    let mut parsed_files = Vec::with_capacity(scanned.len());
    for file in scanned {
        let mut parsed = parser::parse_file_with(
            &file.abs_path,
            &file.rel_path,
            plugins.parser(file.language, &file.abs_path),
            config.include_tests,
        )?;
        // Source identity comes from discovery. Parsers supply the file's syntax facts.
        parsed.path = file.rel_path;
        parsed.language = file.language;
        parsed_files.push(parsed);
    }

    let resolvers = resolver::BuiltinResolvers::new(&config.root);
    let mut file_graph = resolver::resolve_file_graph_with(
        &config.root,
        &parsed_files,
        config.include_mod_decls,
        config.include_tests,
        |file| plugins.resolver(file).unwrap_or_else(|| resolvers.for_file(file)),
    );

    let git_analysis = if config.git.enabled {
        git::analyze_git_history(
            &canonical_root,
            &mut file_graph,
            config.git.lookback_days,
            config.git.max_commits,
        )
    } else {
        None
    };

    analyze_file_graph_with_git(&canonical_root, file_graph, git_analysis, config)
}

/// Analyze an already-constructed `FileGraph` (useful for custom pipelines and unit tests).
pub fn analyze_file_graph(
    root: &std::path::Path,
    file_graph: &FileGraph,
    config: &AnalysisConfig,
) -> Result<AnalysisReport> {
    let mut owned = file_graph.clone();
    domain::projection::assign_auto_module_ids(&mut owned);
    analyze_file_graph_with_git(root, owned, None, config)
}

fn analyze_file_graph_with_git(
    root: &std::path::Path,
    file_graph: FileGraph,
    git: Option<GitAnalysis>,
    config: &AnalysisConfig,
) -> Result<AnalysisReport> {
    let (file_clusters, misplaced_files) =
        domain::algorithms::detect_louvain_and_misplaced(&file_graph);
    let rule_violations =
        domain::rules::evaluate_architecture_rules(&file_graph, &config.layers, &config.rules)?;

    let projections = domain::projection::build_projections(&file_graph, config)?;
    let views: Vec<ArchitectureView> = projections
        .iter()
        .map(|proj| {
            domain::analyzer::analyze_view(
                &file_graph,
                proj,
                config.main_sequence_band,
                config.sdp_tolerance,
                &file_clusters,
                &rule_violations,
            )
        })
        .collect();

    let default_view = views
        .first()
        .map_or_else(|| "modules".to_string(), |v| v.id.clone());

    let check_violations = views
        .first()
        .map_or_default(|v| {
            checks::evaluate_thresholds(v, &config.thresholds, &rule_violations)
        });

    let mut language_counts: BTreeMap<Language, usize> = BTreeMap::new();
    let mut total_loc = 0usize;
    let mut unresolved_import_count = 0usize;

    for f in &file_graph.files {
        *language_counts.entry(f.language).or_insert(0) += 1;
        total_loc += f.loc;
        unresolved_import_count += f.unresolved_imports.len();
    }

    let project_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("repository")
        .to_string();

    Ok(AnalysisReport {
        project_name,
        root_path: root.display().to_string(),
        total_files: file_graph.files.len(),
        total_loc,
        total_internal_edges: file_graph.edges.len(),
        unresolved_import_count,
        language_counts,
        main_sequence_band: config.main_sequence_band,
        sdp_tolerance: config.sdp_tolerance,
        default_view,
        views,
        file_graph,
        git,
        misplaced_files,
        rule_violations,
        baseline_diff: None,
        check_violations,
    })
}
