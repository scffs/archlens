# archlens

Standalone Rust library and CLI architecture intelligence tool that combines static dependency analysis, Clean Architecture metrics, Git behavioral hotspots, declarative architectural rules, and a zero-dependency interactive HTML workbench.

## Features

- **Multi-language AST parsing via Tree-sitter**:
  - **Rust**: `use`, `pub use` re-export chain resolution, local & cross-crate qualified paths, `#[cfg(test)] mod` + `*_tests.rs` exclusion, Rust 2018 module-root pairing (`foo.rs` + `foo/`), composition-root separation, public-first Abstractness, and DIP (`impl Trait`) detection
  - **TypeScript / TSX / JavaScript / JSX / Vue SFC**: `import`, `import type` (DIP), `export ... from`, `require`/`import()`, JSONC `tsconfig.json` `extends` + project `references` (including Nuxt 4 `.nuxt/tsconfig.app.json`), workspace packages
  - **Python** & **Go**: internal package resolution, `Protocol`/`ABC`/`interface` abstraction & symbol extraction
- **Architecture & behavioral metrics**:
  - Clean Architecture **Ca**, **Ce**, **I**, **A**, **D**, **Relational Cohesion H**, **DIP ratio**, **Active Pain** (Zone of Pain weighted by Git hotspot activity)
  - System coupling metrics: **Propagation Cost**, **ACD**, **NCCD**
  - **Tarjan SCC cycles** + **Minimum Feedback Arc Set** cut suggestions
  - **Louvain community detection** + module-level Feature Envy (misplaced files)
  - **Git behavioral analysis**: commit churn, hotspot scores, bus factor (top author share), and **Temporal / Logical Coupling** (hidden co-changes without static imports)
- **Declarative architecture rules & PR diff**:
  - `[[layers]]` strict top-down layering + `[[rules]]` (`forbid`, `allow_only`, `public_api` facade checks, optional `allow_dip`)
  - `--baseline <old.json>` and `archlens diff <old.json> <new.json>` for CI drift tracking
- **Self-contained interactive HTML workbench** (works offline from `file://`):
  - **Compound Sugiyama Layered Graph** (`Crate -> Module -> File -> Symbol` double-click drill-down), **Blast Radius (Ego)** layout, **Cluster Force** layout, minimap, preserved drag positions
  - **Edge Inspector** (exact `file:line`, imported symbols, DIP/cycle/rule badges, `vscode://` links), **Path Finder** (`Shift+Click` A → B), and **Git Temporal Coupling overlay**
  - **Hierarchical Expandable DSM** with SCC cycle boxes
  - **What-If Refactoring Sandbox**: virtually cut edges or move files between modules, watch cycles/SDP/Propagation Cost recompute live, and export a Markdown Action List
  - **Squarified Hotspot Treemap** + **Main Sequence (A/I)** chart + **Diagnostics & Cycle-Breaker** tables

## Installation

```bash
cargo install --git https://github.com/scffs/archlens
```

## Usage

```bash
# Analyze a repository and generate the interactive HTML workbench + JSON snapshot
archlens analyze /path/to/repo --html report.html --json snapshot.json

# Compare current branch against a saved baseline snapshot
archlens analyze /path/to/repo --baseline base.json --html diff-report.html
archlens diff base.json snapshot.json --fail-on-regression

# Run as a CI architectural gate
archlens analyze /path/to/repo --view modules --fail-on-cycles --fail-on-rules --max-propagation-cost 0.45

# Create a starter archlens.toml (layers, rules, git lookback, thresholds)
archlens init /path/to/repo
```
