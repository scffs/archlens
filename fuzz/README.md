# Archlens fuzzing

The `parse_source` target exercises the public parser for Go, Rust, JavaScript, TypeScript, TSX, Python, and Vue. It checks source identity, nonempty lines, and the line bounds of extracted information. The target uses the first byte to select a parser and treats the remaining input as UTF-8 source.

Run the same bounded AddressSanitizer check that CI uses from the repository root:

```sh
rustup toolchain install nightly --profile minimal --component rust-src
cargo +stable install cargo-fuzz --locked --version 0.13.2
cargo +nightly fetch --manifest-path fuzz/Cargo.toml --locked
mkdir -p fuzz/corpus/parse_source
cargo +nightly fuzz run parse_source fuzz/corpus/parse_source fuzz/seeds/parse_source -- -max_total_time=30 -max_len=4096 -verbosity=0 -print_final_stats=1
```

CI discovers every target with `cargo +nightly fuzz list` and requires a seed directory with at least one file per target. Generated inputs go into ignored corpus directories. Each target gets its own 30-second limit.

| Target | Seed input |
| --- | --- |
| `parse_source` | One selector byte followed by UTF-8 source. Seeds cover Go, Rust, JavaScript, TypeScript, TSX, Python, and Vue. |
| `analyze_graph` | The first byte modulo 13 selects how many file descriptor bytes follow. Each descriptor determines a language, folder, and line count. Remaining triples select source and destination indices and import kind. Seeds cover an empty graph, a chain, a cycle, and disconnected components. |

`analyze_graph` generates valid graphs and checks report totals, view edges, DSM membership, cycle membership, and reachability metrics. The reference calculation uses a small reachability matrix.
