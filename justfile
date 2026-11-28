[private]
default:
    @just --list

# Install Rust components and Git hooks for this checkout.
setup:
    rustup toolchain install stable --profile minimal --no-self-update --no-update
    rustup component add --toolchain stable clippy rustfmt
    prek install

# Apply Rust formatting.
fmt:
    cargo +stable fmt --all
    cargo +stable fmt --manifest-path fuzz/Cargo.toml --all

# Check Rust formatting.
fmt-check:
    cargo +stable fmt --all -- --check
    cargo +stable fmt --manifest-path fuzz/Cargo.toml --all -- --check

# Run strict Clippy with the project's configured lints.
lint:
    cargo +stable clippy --locked --all-targets -- -D warnings

# Run the Rust and Bun tests.
test: test-rust test-report

# Run the Rust tests, optionally passing selectors and flags to Cargo.
[positional-arguments]
test-rust *args:
    # Git hooks export repository-specific variables that must not reach temporary test repositories.
    unset $(git rev-parse --local-env-vars) && cargo +stable test --locked "$@"

# Check report asset syntax and run the Bun tests.
test-report:
    bun build --target=browser --no-bundle --outdir target/report-syntax src/report/assets/*.js
    bun test --coverage ./tests/report_model.test.js

# Check the repository for dependency cycles using Archlens itself.
self-check:
    cargo +stable run --locked --quiet -- analyze . --view modules --no-git --fail-on-cycles --quiet

# Run the local quality gate used by the pre-push hook.
check: fmt-check lint test self-check

# Run Archlens from the checkout.
[positional-arguments]
run *args:
    cargo +stable run --locked -- "$@"

# Write line coverage to target/llvm-cov/html/index.html.
coverage:
    cargo +stable llvm-cov --locked --html

# Run cargo-mutants, optionally passing filters.
[positional-arguments]
mutants *args:
    cargo +stable mutants "$@"
