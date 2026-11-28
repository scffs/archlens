# Parser fuzzing

The `parse_source` target exercises the public parser for Go, Rust, JavaScript, TypeScript, TSX, Python, and Vue. It checks source identity, nonempty lines, and the line bounds of extracted information. The target uses the first byte to select a parser and treats the remaining input as UTF-8 source.

Run the same bounded AddressSanitizer check that CI uses from the repository root:

```sh
rustup toolchain install nightly --profile minimal --component rust-src
cargo +stable install cargo-fuzz --locked --version 0.13.2
cargo +nightly fetch --manifest-path fuzz/Cargo.toml --locked
mkdir -p fuzz/corpus/parse_source
cargo +nightly fuzz run parse_source fuzz/corpus/parse_source fuzz/seeds/parse_source -- -max_total_time=30 -max_len=4096 -verbosity=0 -print_final_stats=1
```

The tracked seeds cover all seven parser modes. Generated inputs go into the ignored corpus directory.
