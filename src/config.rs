use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ArchLensError, Result};
use crate::language::Language;

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
#[serde(default)]
pub struct GitConfig {
    pub enabled: bool,
    pub lookback_days: u32,
    pub max_commits: usize,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            lookback_days: 180,
            max_commits: 1500,
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
#[serde(default)]
pub struct AnalysisConfig {
    /// Root directory to scan.
    #[serde(skip)]
    pub root: PathBuf,
    /// Optional language allow-list. Empty means analyze all supported languages.
    pub languages: Vec<Language>,
    /// Extra glob patterns to exclude from scanning (e.g. `**/generated/**`, `**/*.spec.ts`).
    pub exclude: Vec<String>,
    /// Include test files (by default test directories, suffixes, and `#[cfg(test)]` modules are excluded).
    pub include_tests: bool,
    /// Include structural Rust `mod foo;` declarations as dependency edges.
    pub include_mod_decls: bool,
    /// Minimum difference `I(target) - I(source)` required to flag a Stable Dependencies Principle violation.
    pub sdp_tolerance: f64,
    /// Distance threshold separating the Main Sequence band from Zone of Pain / Zone of Uselessness.
    pub main_sequence_band: f64,
    /// Optional custom logical component rules (e.g. Clean Architecture layers or bounded contexts).
    pub components: Vec<ComponentRule>,
    /// Optional strict top-down architecture layers.
    pub layers: Vec<LayerRule>,
    /// Optional declarative boundary/facade rules.
    pub rules: Vec<ArchitectureRule>,
    /// Folder depths to precompute in the report (e.g. `[1, 2, 3]`).
    pub folder_depths: Vec<usize>,
    /// Git behavioral analysis configuration.
    pub git: GitConfig,
    /// Threshold checks for CI gating.
    pub thresholds: ThresholdConfig,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            languages: Vec::new(),
            exclude: [
                "**/node_modules/**",
                "**/target/**",
                "**/.git/**",
                "**/dist/**",
                "**/build/**",
                "**/.next/**",
                "**/.nuxt/**",
                "**/.output/**",
                "**/coverage/**",
                "**/playwright-report/**",
                "**/test-results/**",
                "**/__pycache__/**",
                "**/.venv/**",
                "**/venv/**",
                "**/vendor/**",
                "**/.pnpm-store/**",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            include_tests: false,
            include_mod_decls: false,
            sdp_tolerance: 0.05,
            main_sequence_band: 0.30,
            components: Vec::new(),
            layers: Vec::new(),
            rules: Vec::new(),
            folder_depths: vec![1, 2, 3],
            git: GitConfig::default(),
            thresholds: ThresholdConfig::default(),
        }
    }
}

impl AnalysisConfig {
    /// Create a default configuration targeting `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            ..Self::default()
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
