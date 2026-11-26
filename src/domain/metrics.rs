use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::model::{FileGraph, ImportKind, Language, SymbolCounts};
use crate::git::GitAnalysis;

/// Classification of a component on Robert C. Martin's Abstractness vs Instability chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureZone {
    /// Balanced component near the diagonal `A + I = 1`.
    MainSequence,
    /// Concrete and heavily depended upon (`A + I < 1 - band`): painful to change.
    ZoneOfPain,
    /// Abstract with few or no dependents (`A + I > 1 + band`): dead or over-engineered abstractions.
    ZoneOfUselessness,
    /// No incoming or outgoing internal dependencies (`Ca == 0 && Ce == 0`).
    Isolated,
    /// Contains no countable abstract/concrete declarations (e.g. constants, barrel re-exports).
    NoEntities,
}

/// Concrete file-level dependency detail attached to aggregated component edges and cycles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedFileEdge {
    pub from: String,
    pub to: String,
    pub line: usize,
    #[serde(default)]
    pub lines: Vec<usize>,
    #[serde(default)]
    pub symbols: Vec<String>,
    pub kind: ImportKind,
    #[serde(default)]
    pub is_dip: bool,
}

/// Clean Architecture coupling, stability, and behavioral metrics for one node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetrics {
    /// Identifier of this component/folder/file in the current view.
    pub id: String,
    /// Display label.
    pub label: String,
    /// Number of source files aggregated into this node.
    pub file_count: usize,
    /// Total non-empty lines of code.
    pub loc: usize,
    /// Primary language (or `"Mixed"` when multiple languages coexist in the component).
    pub primary_language: String,
    /// Per-language file counts.
    pub languages: BTreeMap<Language, usize>,
    /// Files belonging to this component.
    pub files: Vec<String>,
    /// Count of abstract vs concrete declarations inside this component.
    pub symbols: SymbolCounts,
    /// Afferent coupling (Fan-in): distinct external files depending on files inside this node.
    pub afferent_coupling: usize,
    /// Efferent coupling (Fan-out): distinct external files depended on by files inside this node.
    pub efferent_coupling: usize,
    /// Number of distinct external components in this view depending on this node.
    pub component_fan_in: usize,
    /// Number of distinct external components in this view that this node depends on.
    pub component_fan_out: usize,
    /// Distinct external third-party packages referenced by files in this node.
    pub external_dep_count: usize,
    /// Directed file-level edges completely internal to this node.
    pub internal_edges: usize,
    /// Relational cohesion `(internal_edges + 1) / file_count` (when `file_count > 0`).
    pub cohesion: f64,
    /// Instability `I = Ce / (Ca + Ce)` in `[0.0, 1.0]`, or `None` when isolated (`Ca + Ce == 0`).
    pub instability: Option<f64>,
    /// Abstractness `A = Na / Nc` in `[0.0, 1.0]`, or `None` when `Nc == 0`.
    pub abstractness: Option<f64>,
    /// Distance from the Main Sequence `D = |A + I - 1|`, or `None` when `I` or `A` is undefined.
    pub distance: Option<f64>,
    /// Zone classification on the Main Sequence chart.
    pub zone: ArchitectureZone,
    /// Whether this node participates in at least one dependency cycle (SCC size >= 2).
    pub in_cycle: bool,
    /// Share of incident file edges that go through DIP contracts (`impl Trait`, `type-only`, abstract ports).
    #[serde(default)]
    pub dip_ratio: f64,
    /// Aggregated Git commit touches across files in this node.
    #[serde(default)]
    pub commits: usize,
    /// Aggregated Git churn (lines added + deleted) across files in this node.
    #[serde(default)]
    pub churn: usize,
    /// Max hotspot score among files in this node (`0.0..=1.0`).
    #[serde(default)]
    pub hotspot_score: f64,
    /// Primary author commit share (`0.0..=1.0`, higher = higher bus-factor risk).
    #[serde(default)]
    pub top_author_share: f64,
    /// Primary contributor name when Git history is available.
    #[serde(default)]
    pub top_author: String,
    /// True if node is in Zone of Pain AND actively changing in Git (not benign stable core).
    #[serde(default)]
    pub active_pain: bool,
    /// Dominant Louvain community ID detected from actual coupling structure.
    #[serde(default)]
    pub louvain_cluster: usize,
}

/// Directed aggregated dependency edge between two nodes in a view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewEdge {
    pub from: String,
    pub to: String,
    /// Number of underlying file-to-file edges collapsed into this component edge.
    pub weight: usize,
    /// Detailed file-level edges causing this dependency.
    #[serde(default)]
    pub file_edges: Vec<DetailedFileEdge>,
    /// Compact `(from_file, to_file)` pairs kept for backwards compatibility.
    pub sample_file_edges: Vec<(String, String)>,
    /// True if both endpoints belong to the same dependency cycle (SCC).
    pub is_cycle_edge: bool,
    /// True if `I(from) < I(to) - tolerance` (violates Stable Dependencies Principle).
    pub is_sdp_violation: bool,
    /// Number of underlying file edges that are DIP (trait impl / type-only / abstract ports).
    #[serde(default)]
    pub dip_weight: usize,
    /// True if this aggregated edge violates at least one configured architecture rule.
    #[serde(default)]
    pub is_rule_violation: bool,
}

/// Minimal edge removal recommended to break a strongly connected component cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleBreakEdge {
    pub from: String,
    pub to: String,
    pub weight: usize,
    pub file_edges: Vec<DetailedFileEdge>,
}

/// A strongly connected component of size >= 2 representing a circular dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleGroup {
    pub nodes: Vec<String>,
    /// Concrete aggregated edges inside the cycle.
    pub edge_count: usize,
    /// Sample file-level edges participating in this cycle.
    pub sample_file_edges: Vec<(String, String)>,
    /// Minimal weighted feedback arc set: cutting these edges breaks the entire cycle.
    #[serde(default)]
    pub break_suggestions: Vec<CycleBreakEdge>,
}

/// A violation of Robert C. Martin's Stable Dependencies Principle (`I(from) < I(to)`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdpViolation {
    pub from: String,
    pub to: String,
    pub from_instability: f64,
    pub to_instability: f64,
    pub delta: f64,
    pub weight: usize,
    /// True when all underlying file edges are DIP (e.g. `impl Trait` adapter), reducing severity.
    #[serde(default)]
    pub is_dip_only: bool,
}

/// A file whose actual coupling affinity (Louvain cluster) strongly belongs to another module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MisplacedFile {
    pub file: String,
    pub current_module: String,
    pub suggested_module: String,
    pub internal_edges: usize,
    pub suggested_module_edges: usize,
    pub affinity: f64,
}

/// Violation of a declarative architectural boundary or layering rule from `archlens.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleViolation {
    pub rule_name: String,
    pub kind: String,
    pub from_file: String,
    pub to_file: String,
    pub from_module: String,
    pub to_module: String,
    pub line: usize,
    pub symbols: Vec<String>,
    pub message: String,
}

/// Delta comparison against a previous baseline `AnalysisReport` JSON snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineDiff {
    pub baseline_view: String,
    pub delta_nodes: isize,
    pub delta_edges: isize,
    pub delta_cycles: isize,
    pub delta_sdp_violations: isize,
    pub delta_rule_violations: isize,
    pub delta_propagation_cost: f64,
    pub delta_average_distance: Option<f64>,
    pub new_cycles: Vec<Vec<String>>,
    pub resolved_cycles: Vec<Vec<String>>,
    pub new_edges: Vec<(String, String)>,
    pub removed_edges: Vec<(String, String)>,
    pub new_rule_violations: Vec<RuleViolation>,
}

/// High-level summary counters for an architecture view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewSummary {
    pub node_count: usize,
    pub edge_count: usize,
    pub cycle_count: usize,
    pub nodes_in_cycles: usize,
    pub sdp_violation_count: usize,
    pub zone_of_pain_count: usize,
    #[serde(default)]
    pub active_pain_count: usize,
    pub zone_of_uselessness_count: usize,
    pub main_sequence_count: usize,
    pub isolated_count: usize,
    pub no_entities_count: usize,
    pub average_distance: Option<f64>,
    pub max_distance: Option<f64>,
    pub average_instability: Option<f64>,
    pub average_abstractness: Option<f64>,
    /// Transitive propagation cost (`0.0..=1.0`): share of system reachable on average.
    #[serde(default)]
    pub propagation_cost: f64,
    /// Lakos Average Component Dependency (mean transitive dependencies + 1).
    #[serde(default)]
    pub acd: f64,
    /// Lakos Normalized Cumulative Component Dependency (relative to balanced binary tree).
    #[serde(default)]
    pub nccd: f64,
    #[serde(default)]
    pub rule_violation_count: usize,
}

/// Complete analyzed view at one granularity (e.g. `modules`, `packages`, `folders-d2`, `files`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub nodes: Vec<NodeMetrics>,
    pub edges: Vec<ViewEdge>,
    pub cycles: Vec<CycleGroup>,
    pub sdp_violations: Vec<SdpViolation>,
    /// Node IDs ordered for a Dependency Structure Matrix (DSM): stable/foundational nodes first,
    /// SCC clusters kept adjacent so feedback edges clearly appear above the diagonal.
    pub dsm_order: Vec<String>,
    pub summary: ViewSummary,
}

/// Result of evaluating CI threshold or architectural boundary rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckViolation {
    pub rule: String,
    pub message: String,
}

/// Top-level analysis report returned by `archlens::analyze` and serialized to JSON/HTML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub project_name: String,
    pub root_path: String,
    pub total_files: usize,
    pub total_loc: usize,
    pub total_internal_edges: usize,
    pub unresolved_import_count: usize,
    pub language_counts: BTreeMap<Language, usize>,
    pub main_sequence_band: f64,
    pub sdp_tolerance: f64,
    /// Primary view key used by default in CLI summaries and HTML initial state.
    pub default_view: String,
    /// All computed granularity views (`components`/`modules`, `packages`, `folders-d*`, `files`).
    pub views: Vec<ArchitectureView>,
    /// Complete file-level dependency graph (with declarations, symbols, lines, and Git metrics)
    /// powering compound drill-down, hierarchical DSM, path finder, and What-If simulations.
    #[serde(default)]
    pub file_graph: FileGraph,
    /// Optional Git churn, hotspot, and temporal coupling data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitAnalysis>,
    /// Files whose coupling structure (Louvain community) suggests another module.
    #[serde(default)]
    pub misplaced_files: Vec<MisplacedFile>,
    /// Violations of declarative architecture rules defined in `archlens.toml`.
    #[serde(default)]
    pub rule_violations: Vec<RuleViolation>,
    /// Optional delta comparison against a baseline report JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_diff: Option<BaselineDiff>,
    /// Threshold and architectural rule check failures for CI gating.
    pub check_violations: Vec<CheckViolation>,
}
