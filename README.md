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

## Extending the library

Build an `AnalysisPlugins` value and pass it to `analyze_with_plugins`. Each
analysis receives its own registrations.

- Implement `parser::LanguageParser` to extract imports and declarations.
  `register_source` attaches a parser to one extension. Use `register_parser`
  to replace a parser for an existing report language.
- Implement `resolver::ImportResolver` to resolve regular imports and
  re-exports. Register it for an extension or a report language.
  `register_language` installs a parser, resolver, and extension list together.
- Implement `ViewProvider` to append a projection with a unique ID.
  `analyze_file_graph_with_plugins` also accepts view providers for graphs
  constructed by another tool.

A source registration uses an existing `Language` ID in reports. Adding a new
built-in report language also requires an ID in the language inventory and
bindings for the built-in parser, resolver, and CLI. The public parser and
resolver contracts can be used by custom integrations without changing the
analysis pipeline.

## Development

With [mise](https://mise.jdx.dev/) and [rustup](https://rustup.rs/) installed, run
these commands from the checkout. On Windows, use Git Bash.

```sh
mise trust
mise install
mise exec -- just setup
mise exec -- just check
```

`just setup` installs the stable Rust components and the repository's Git hooks.
Prek installs both hooks from `.pre-commit-config.yaml`. The pre-commit hook
checks formatting for Rust changes and runs Bun tests for report changes. The
pre-push hook runs the local quality gate.

Use `mise exec -- just` to list recipes. To analyze a project with the checkout,
run `mise exec -- just -- run analyze path/to/project --html report.html`.

To generate a local Rust coverage report, install the coverage tool once, then
open `target/llvm-cov/html/index.html` after the run.

```sh
rustup component add --toolchain stable llvm-tools-preview
cargo +stable install cargo-llvm-cov --locked --version 0.9.1
mise exec -- just coverage
```

## Mutation testing

The Robustness workflow tests mutations of the Rust code changed by a pull
request. Its weekly and manual runs test mutations across the whole crate in
eight shards. Each shard saves the verdicts and logs as an Actions artifact,
including when a mutant survives or times out. Run the full check on a branch
that changes only tests to see whether its assertions still catch the same
mutants.

With [cargo-mutants](https://mutants.rs/) installed, run the full check with
`cargo +stable mutants` or select a module with
`cargo +stable mutants --file 'src/domain/**'`. Review `mutants.out/missed.txt`
for survived mutations and the corresponding diffs and test logs for the
behavior to assert. To check a test's individual contribution, rerun its
mutants with the named test skipped and compare the verdicts.
