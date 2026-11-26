use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::model::Language;
use crate::error::{ArchLensError, Result};

/// User-defined rule mapping file paths (glob patterns) into a named logical component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRule {
    pub name: String,
    pub patterns: Vec<String>,
}

/// Kind of declarative architectural boundary rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// Files matching `from` must not import files matching `to`.
    Forbid,
    /// Files matching `from` may only import internal files matching `to` (or within `from`).
    AllowOnly,
    /// External callers outside `from` may only import `from` files matching `to` (facade enforcement).
    PublicApi,
}

/// Declarative architectural boundary rule evaluated against every internal file edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureRule {
    pub name: String,
    pub kind: RuleKind,
    /// Glob patterns matching source files (or protected module files for `public_api`).
    pub from: Vec<String>,
    /// Glob patterns matching target files (or allowed public entrypoints for `public_api`).
    pub to: Vec<String>,
    /// If true, edges satisfying Dependency Inversion (`impl Trait`, `type-only`, abstract ports) are allowed.
    #[serde(default)]
    pub allow_dip: bool,
    /// Optional human-readable rationale printed on violations.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Ordered layer definition (top-most layer first; lower layers cannot depend on higher layers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerRule {
    pub name: String,
    pub patterns: Vec<String>,
    /// Allow upward edges that only depend on traits/interfaces (DIP).
    #[serde(default)]
    pub allow_dip: bool,
}

/// Git behavioral history configuration (churn, hotspots, temporal coupling).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_git_lookback_days")]
    pub lookback_days: u32,
    #[serde(default = "default_git_max_commits")]
    pub max_commits: usize,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            lookback_days: default_git_lookback_days(),
            max_commits: default_git_max_commits(),
        }
    }
}

/// Optional CI quality-gate thresholds evaluated against a chosen architecture view.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThresholdConfig {
    /// Fail if any dependency cycle (SCC of size >= 2) exists.
    #[serde(default)]
    pub fail_on_cycles: bool,
    /// Fail if any declarative architecture rule or layer violation exists.
    #[serde(default)]
    pub fail_on_rule_violations: bool,
    /// Fail if any component's distance `D` from the Main Sequence exceeds this value (0.0..=1.0).
    pub max_distance: Option<f64>,
    /// Fail if the total number of SDP violations exceeds this count.
    pub max_sdp_violations: Option<usize>,
    /// Fail if the number of components in the Zone of Pain exceeds this count.
    pub max_zone_of_pain: Option<usize>,
    /// Fail if propagation cost exceeds this fraction (`0.0..=1.0`).
    pub max_propagation_cost: Option<f64>,
}

/// Configuration for running an architectural analysis on a codebase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Root directory to scan.
    #[serde(skip)]
    pub root: PathBuf,
    /// Optional language allow-list. Empty means analyze all supported languages.
    #[serde(default)]
    pub languages: Vec<Language>,
    /// Extra glob patterns to exclude from scanning (e.g. `**/generated/**`, `**/*.spec.ts`).
    #[serde(default = "default_excludes")]
    pub exclude: Vec<String>,
    /// Include test files (by default test directories, suffixes, and `#[cfg(test)]` modules are excluded).
    #[serde(default)]
    pub include_tests: bool,
    /// Include structural Rust `mod foo;` declarations as dependency edges.
    #[serde(default)]
    pub include_mod_decls: bool,
    /// Minimum difference `I(target) - I(source)` required to flag a Stable Dependencies Principle violation.
    #[serde(default = "default_sdp_tolerance")]
    pub sdp_tolerance: f64,
    /// Distance threshold separating the Main Sequence band from Zone of Pain / Zone of Uselessness.
    #[serde(default = "default_main_sequence_band")]
    pub main_sequence_band: f64,
    /// Optional custom logical component rules (e.g. Clean Architecture layers or bounded contexts).
    #[serde(default)]
    pub components: Vec<ComponentRule>,
    /// Optional strict top-down architecture layers.
    #[serde(default)]
    pub layers: Vec<LayerRule>,
    /// Optional declarative boundary/facade rules.
    #[serde(default)]
    pub rules: Vec<ArchitectureRule>,
    /// Folder depths to precompute in the report (e.g. `[1, 2, 3]`).
    #[serde(default = "default_folder_depths")]
    pub folder_depths: Vec<usize>,
    /// Git behavioral analysis configuration.
    #[serde(default)]
    pub git: GitConfig,
    /// Threshold checks for CI gating.
    #[serde(default)]
    pub thresholds: ThresholdConfig,
}

fn default_true() -> bool {
    true
}

fn default_git_lookback_days() -> u32 {
    180
}

fn default_git_max_commits() -> usize {
    1500
}

fn default_excludes() -> Vec<String> {
    vec![
        "**/node_modules/**".to_string(),
        "**/target/**".to_string(),
        "**/.git/**".to_string(),
        "**/dist/**".to_string(),
        "**/build/**".to_string(),
        "**/.next/**".to_string(),
        "**/.nuxt/**".to_string(),
        "**/.output/**".to_string(),
        "**/coverage/**".to_string(),
        "**/playwright-report/**".to_string(),
        "**/test-results/**".to_string(),
        "**/__pycache__/**".to_string(),
        "**/.venv/**".to_string(),
        "**/venv/**".to_string(),
        "**/vendor/**".to_string(),
        "**/.pnpm-store/**".to_string(),
    ]
}

fn default_sdp_tolerance() -> f64 {
    0.05
}

fn default_main_sequence_band() -> f64 {
    0.30
}

fn default_folder_depths() -> Vec<usize> {
    vec![1, 2, 3]
}

impl AnalysisConfig {
    /// Create a default configuration targeting `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            languages: Vec::new(),
            exclude: default_excludes(),
            include_tests: false,
            include_mod_decls: false,
            sdp_tolerance: default_sdp_tolerance(),
            main_sequence_band: default_main_sequence_band(),
            components: Vec::new(),
            layers: Vec::new(),
            rules: Vec::new(),
            folder_depths: default_folder_depths(),
            git: GitConfig::default(),
            thresholds: ThresholdConfig::default(),
        }
    }

    /// Load configuration from a TOML file (`archlens.toml`) and bind it to `root`.
    pub fn from_toml_file(path: &Path, root: impl Into<PathBuf>) -> Result<Self> {
        let raw = std::fs::read_to_string(path).map_err(|source| ArchLensError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut cfg: Self = toml::from_str(&raw).map_err(|source| ArchLensError::ConfigParse {
            path: path.to_path_buf(),
            source,
        })?;
        cfg.root = root.into();
        Ok(cfg)
    }

    /// Load `archlens.toml` from `root` if it exists; otherwise return default config.
    pub fn load_or_default(root: impl Into<PathBuf>) -> Result<Self> {
        let root_path = root.into();
        let candidate = root_path.join("archlens.toml");
        if candidate.is_file() {
            Self::from_toml_file(&candidate, root_path)
        } else {
            Ok(Self::new(root_path))
        }
    }
}
