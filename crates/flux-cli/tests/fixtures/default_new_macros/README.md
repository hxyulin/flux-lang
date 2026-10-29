# default_new_macros 0.1.0 fixture

`Cargo.toml` and `src/lib.rs` are unchanged files from the published
`default_new_macros 0.1.0` package. The source retains its original CRLF
line endings. Do not run rustfmt on this fixture.

- Package: <https://crates.io/crates/default_new_macros/0.1.0>
- Archive: <https://static.crates.io/crates/default_new_macros/default_new_macros-0.1.0.crate>
- Archive SHA-256: `30b5d4163a12d446067eec290deed29f0cf849338cd974e5930f15e2d6193297`.
- Source SHA-256: `ceae0db7d9aa8a2d6fffe422cb766e8a50fe1e377a3972957f8de0879075975b`.
- Upstream repository: <https://github.com/recatek/default_new>.
- Package revision: `328b983294bb7b2a7aa0dbee2dba73094fa97640`, directory `macros`.

The package declares `MIT OR Apache-2.0`. The accompanying license files
come from that upstream revision's repository root.

The focused `published_default_new_derive_matches_rustc` test copies these
files to a temporary standalone package. It compares the generated program's
output between rustc's native proc-macro implementation and Flux's token API
host. This demonstrates one published derive, not compatibility with the
`syn`, `quote` or `proc-macro2` dependency graph.
