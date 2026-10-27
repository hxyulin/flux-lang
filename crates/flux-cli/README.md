# flux-cli

The `flux` command for [flux](../../README.md), an experimental compiler for
a subset of Rust.

```sh
flux lex file.rs      # tokens, trivia included
flux parse file.rs    # syntax tree and errors
flux check a.rs b.rs  # errors only; exits with 1 if there are any
```


## Cargo-backed source builds

```sh
flux cargo prepare --manifest-path path/to/Cargo.toml -p application --offline
flux cargo build --manifest-path path/to/Cargo.toml -p application --offline
flux cargo build --manifest-path path/to/Cargo.toml -p application --offline -O
```

These commands always use the lockfile. Select one binary with `--bin` if the
package has several. `--features` and `--no-default-features` are forwarded to
Cargo. A virtual workspace requires `-p` unless it has one default member.
The existing `flux package` driver retains its smaller, standalone contract.

Cargo resolves the graph, checks the Rust target sources, runs build scripts,
and compiles host proc macros. Flux then compiles original target source roots,
including generated Rust includes, with actual crate editions, feature sets,
build-script cfgs, rustc-env entries, and separate OUT_DIR values. Rustc target
artifacts are never linked into the Flux executable. Cargo check is a preparation
step, not evidence that Flux can compile that graph.

Preparation writes `target/flux/cargo-plan.json`, or `--plan <file>`. Version 1
records source identities and dependency aliases, environments, cfgs, host
proc-macro artifact paths, build-script linker directives, and blockers. The
rustc version is recorded because host macro artifacts depend on its ABI. Cargo
may reuse cached build-script outputs and artifacts; preparation does not force
them to rerun. The plan contains explicit compile-time values and local paths.

Supported builds currently select one host binary on arm64 macOS. Proc-macro
**preparation** works, including real derive crates, but **expansion in Flux** is
not implemented. `prepare` reports that in its plan and succeeds; `build` reports
it as an error before Flux compilation. Build-script native linker directives and
distinct host/target script units also require further integration. Those are
rejected rather than ignored. There is no cross-compilation, unit-graph feature
reconciliation, Cargo profile mapping, or library artifact export yet.

Explicit source environments contain common Cargo package/crate variables and
build-script outputs. Arbitrary process variables are not implicitly serialized
or inherited. Direct standalone `flux build file.rs` retains process environment
lookup. This avoids leaking unrelated process values into source-graph plans;
projects requiring other `env!` inputs need an explicit input interface extension.

This uses Cargo's documented [metadata and JSON messages](https://doc.rust-lang.org/cargo/reference/external-tools.html)
and [build-script outputs](https://doc.rust-lang.org/cargo/reference/build-scripts.html).
A host proc-macro artifact still needs the compiler-side token/spans interface
required by [procedural macros](https://doc.rust-lang.org/reference/procedural-macros.html).
