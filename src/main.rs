use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

use archlens::checks::evaluate_thresholds;
use archlens::config::AnalysisConfig;
use archlens::diff::compute_baseline_diff;
use archlens::domain::model::Language;
use archlens::report::html::write_html_report;
use archlens::report::json::{load_report_json, write_json_report};
use archlens::report::terminal::{render_diff_summary, render_terminal_summary};

#[derive(Debug, Parser)]
#[command(
    name = "archlens",
    version,
    about = "Architecture intelligence: compound dependency graphs, Clean Architecture metrics, Git hotspots, rules & DSM"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Analyze a codebase and generate terminal, JSON, and/or interactive HTML reports.
    Analyze(Box<AnalyzeArgs>),
    /// Compare two saved JSON reports to inspect architectural drift between commits or PRs.
    Diff {
        /// Path to baseline JSON report.
        baseline: PathBuf,
        /// Path to current JSON report.
        current: PathBuf,
        /// Optional view ID to compare (defaults to current report's default view).
        #[arg(long)]
        view: Option<String>,
        /// Fail with exit code 1 if new cycles or new rule violations were introduced.
        #[arg(long)]
        fail_on_regression: bool,
    },
    /// Write a starter `archlens.toml` configuration file in the target directory.
    Init {
        /// Target directory (defaults to current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliLanguage {
    Rust,
    Ts,
    Js,
    Vue,
    Python,
    Go,
}

impl From<CliLanguage> for Language {
    fn from(val: CliLanguage) -> Self {
        match val {
            CliLanguage::Rust => Self::Rust,
            CliLanguage::Ts => Self::TypeScript,
            CliLanguage::Js => Self::JavaScript,
            CliLanguage::Vue => Self::Vue,
            CliLanguage::Python => Self::Python,
            CliLanguage::Go => Self::Go,
        }
    }
}

#[derive(Debug, Args)]
struct AnalyzeArgs {
    /// Path to repository or subdirectory to analyze.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Optional path to `archlens.toml` config file.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Output path for the self-contained interactive HTML report.
    #[arg(long)]
    html: Option<PathBuf>,

    /// Output path for the machine-readable JSON report.
    #[arg(long)]
    json: Option<PathBuf>,

    /// Optional baseline JSON report to compute PR/commit architectural delta.
    #[arg(long)]
    baseline: Option<PathBuf>,

    /// Granularity view for terminal summary and threshold checks (e.g. `modules`, `packages`, `folders-d2`, `files`, `components`).
    #[arg(long)]
    view: Option<String>,

    /// Restrict analysis to specific languages (comma-separated or repeated).
    #[arg(long = "lang", value_delimiter = ',')]
    languages: Vec<CliLanguage>,

    /// Extra glob patterns to exclude.
    #[arg(long = "exclude")]
    excludes: Vec<String>,

    /// Include test files and `#[cfg(test)]` modules.
    #[arg(long)]
    include_tests: bool,

    /// Include Rust structural `mod foo;` declarations as dependency edges.
    #[arg(long)]
    include_mod_decls: bool,

    /// Disable Git churn, hotspot, and temporal coupling analysis.
    #[arg(long)]
    no_git: bool,

    /// Git history lookback window in days (default: 180).
    #[arg(long)]
    git_days: Option<u32>,

    /// Main Sequence band width (default 0.30).
    #[arg(long)]
    band: Option<f64>,

    /// Minimum instability delta to flag an SDP violation (default 0.05).
    #[arg(long)]
    sdp_tolerance: Option<f64>,

    /// Exit with code 1 if any dependency cycle exists in the selected view.
    #[arg(long)]
    fail_on_cycles: bool,

    /// Exit with code 1 if any declarative architecture rule is violated.
    #[arg(long)]
    fail_on_rules: bool,

    /// Exit with code 1 if any component has distance D above this value (0.0..=1.0).
    #[arg(long)]
    max_distance: Option<f64>,

    /// Exit with code 1 if SDP violations exceed this count.
    #[arg(long)]
    max_sdp_violations: Option<usize>,

    /// Exit with code 1 if components in Zone of Pain exceed this count.
    #[arg(long)]
    max_zone_of_pain: Option<usize>,

    /// Exit with code 1 if Propagation Cost exceeds this fraction (0.0..=1.0).
    #[arg(long)]
    max_propagation_cost: Option<f64>,

    /// Suppress terminal summary output.
    #[arg(short, long)]
    quiet: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> archlens::Result<ExitCode> {
    match cli.command {
        Commands::Init { path } => run_init(&path),
        Commands::Analyze(args) => run_analyze(*args),
        Commands::Diff {
            baseline,
            current,
            view,
            fail_on_regression,
        } => run_diff(&baseline, &current, view.as_deref(), fail_on_regression),
    }
}

fn run_diff(
    baseline_path: &Path,
    current_path: &Path,
    view: Option<&str>,
    fail_on_regression: bool,
) -> archlens::Result<ExitCode> {
    let base = load_report_json(baseline_path)?;
    let cur = load_report_json(current_path)?;
    if let Some(diff) = compute_baseline_diff(&base, &cur, view) {
        print!("{}", render_diff_summary(&diff));
        let regressed = !diff.new_cycles.is_empty() || !diff.new_rule_violations.is_empty();
        if fail_on_regression && regressed {
            return Ok(ExitCode::from(1));
        }
    } else {
        println!("No overlapping views to compare.");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_init(dir: &Path) -> archlens::Result<ExitCode> {
    let target = dir.join("archlens.toml");
    if target.exists() {
        eprintln!("{} already exists; leaving it untouched.", target.display());
        return Ok(ExitCode::from(1));
    }
    let starter = r#"# archlens configuration (optional — works with zero config out of the box)

include_tests = false
main_sequence_band = 0.30
sdp_tolerance = 0.05
folder_depths = [1, 2, 3]

exclude = [
  "**/node_modules/**",
  "**/target/**",
  "**/dist/**",
  "**/generated/**",
]

[git]
enabled = true
lookback_days = 180
max_commits = 1500

# Optional: strict top-down Clean Architecture layers (lower cannot depend on upper)
# [[layers]]
# name = "api"
# patterns = ["api/**"]
#
# [[layers]]
# name = "application"
# patterns = ["app/src/application/**"]
#
# [[layers]]
# name = "domain"
# patterns = ["app/src/domain/**"]

# Optional: explicit boundary & facade rules (`forbid`, `allow_only`, `public_api`)
# [[rules]]
# name = "domain-purity"
# kind = "forbid"
# from = ["**/domain/**"]
# to = ["**/infra/**", "**/api/**"]
# reason = "Domain layer must stay free of infrastructure and transport dependencies"

[thresholds]
fail_on_cycles = false
fail_on_rule_violations = false
# max_distance = 0.75
# max_sdp_violations = 5
# max_zone_of_pain = 2
# max_propagation_cost = 0.50
"#;
    std::fs::write(&target, starter).map_err(|source| archlens::ArchLensError::Io {
        path: target.clone(),
        source,
    })?;
    println!("Created {}", target.display());
    Ok(ExitCode::SUCCESS)
}

fn run_analyze(args: AnalyzeArgs) -> archlens::Result<ExitCode> {
    let mut cfg = match &args.config {
        Some(cfg_path) => AnalysisConfig::from_toml_file(cfg_path, &args.path)?,
        None => AnalysisConfig::load_or_default(&args.path)?,
    };

    if !args.languages.is_empty() {
        cfg.languages = args.languages.into_iter().map(Language::from).collect();
    }
    if !args.excludes.is_empty() {
        cfg.exclude.extend(args.excludes);
    }
    if args.include_tests {
        cfg.include_tests = true;
    }
    if args.include_mod_decls {
        cfg.include_mod_decls = true;
    }
    if args.no_git {
        cfg.git.enabled = false;
    }
    if let Some(days) = args.git_days {
        cfg.git.lookback_days = days;
    }
    if let Some(band) = args.band {
        cfg.main_sequence_band = band;
    }
    if let Some(tol) = args.sdp_tolerance {
        cfg.sdp_tolerance = tol;
    }
    if args.fail_on_cycles {
        cfg.thresholds.fail_on_cycles = true;
    }
    if args.fail_on_rules {
        cfg.thresholds.fail_on_rule_violations = true;
    }
    if let Some(md) = args.max_distance {
        cfg.thresholds.max_distance = Some(md);
    }
    if let Some(msdp) = args.max_sdp_violations {
        cfg.thresholds.max_sdp_violations = Some(msdp);
    }
    if let Some(mpain) = args.max_zone_of_pain {
        cfg.thresholds.max_zone_of_pain = Some(mpain);
    }
    if let Some(mpc) = args.max_propagation_cost {
        cfg.thresholds.max_propagation_cost = Some(mpc);
    }

    let mut report = archlens::analyze(&cfg)?;

    if let Some(ref view_id) = args.view {
        if let Some(view) = report.view(view_id) {
            let violations = evaluate_thresholds(view, &cfg.thresholds, &report.rule_violations);
            report.default_view.clone_from(view_id);
            report.check_violations = violations;
        } else {
            let available: Vec<&str> = report.views.iter().map(|v| v.id.as_str()).collect();
            eprintln!(
                "warning: view '{view_id}' not found; available views: {}",
                available.join(", ")
            );
        }
    }

    if let Some(ref baseline_path) = args.baseline {
        let base = load_report_json(baseline_path)?;
        report.baseline_diff = compute_baseline_diff(&base, &report, args.view.as_deref());
    }

    if let Some(ref html_path) = args.html {
        write_html_report(&report, html_path)?;
        if !args.quiet {
            println!("HTML report written to {}", html_path.display());
        }
    }

    if let Some(ref json_path) = args.json {
        write_json_report(&report, json_path)?;
        if !args.quiet {
            println!("JSON report written to {}", json_path.display());
        }
    }

    if !args.quiet {
        print!("{}", render_terminal_summary(&report, args.view.as_deref()));
    }

    if report.check_violations.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(1))
    }
}
