# flux-cli

The `flux` command for [flux](../../README.md), an experimental compiler for
a subset of Rust.

```sh
flux lex file.rs      # tokens, trivia included
flux parse file.rs    # syntax tree and errors
flux check a.rs b.rs  # errors only; exits with 1 if there are any
```


## Native offline planning

```sh
flux cargo plan --manifest-path Cargo.toml -p flux-cli --bin flux
```

This command reads manifests and Cargo.lock directly. It never invokes Cargo,
accesses the network, runs build scripts, or loads proc macros. Dependencies must
already exist in `CARGO_HOME` (default `~/.cargo`). Rustc supplies the host triple
and target cfg; Git checks cached checkout revisions. Planning currently targets
the host platform. `--offline` is accepted but planning is always offline.

The separate version 1 `flux-native-offline` plan defaults to
`target/flux/native-plan.json`; use `--plan` to choose another path. It records
locked package identities, checksums, manifest paths, and dependency-first build
units. Host and target features stay separate under resolver 2/3. Optional
dependencies, dependency aliases, workspace inheritance, default features,
`dep:` activation, strong and weak feature forwarding, and target predicates
are supported. Build scripts record both their host compilation domain and the
feature domain of the package they prepare. Proc macros are host units.

The source cache is trusted as a working tree. Registry package checksum
metadata is compared with the lockfile when present; extracted source files
and git working-tree changes are not integrity-checked. Git sources must have
the exact locked commit checked out. Missing or ambiguous sources and stale
lockfile edges fail without fetching or updating Cargo.lock.

Planning does not execute the plan or produce the prepared artifacts expected
by `flux cargo build`. Host-tool execution is the next step toward Cargo-free
builds. Alternate registries, source replacement, patch/replace overrides,
workspace member globs, resolver 1, and cross compilation are not supported.
Unsupported source/target overrides in Cargo configuration fail explicitly.

## Cargo-backed source builds

```sh
flux cargo prepare --manifest-path path/to/Cargo.toml -p application --offline
flux cargo build --manifest-path path/to/Cargo.toml -p application --offline
flux cargo build --manifest-path path/to/Cargo.toml -p application --offline -O
```

These commands always use the lockfile. Select one binary with `--bin` if the
package has several. `--features` and `--no-default-features` are forwarded to
Cargo. A virtual workspace requires `-p` unless it has one default member.
The `flux package` driver reads manifests directly without invoking Cargo,
under its smaller, standalone contract.
It can build local macro hosts with direct rustc calls or dependency-free
hosts with `--macro-host-compiler flux`; see
[local macro hosts](../../docs/PACKAGE_DRIVER.md#local-procedural-macro-hosts).
Both host routes implement the same HIR expansion interface. The Cargo-backed
host protocol and lifecycle described here retain their existing behavior.

Cargo resolves the graph, checks the Rust target sources, runs build scripts,
and compiles host proc macros. Flux then compiles original target source roots,
including generated Rust includes, with actual crate editions, feature sets,
build-script cfgs, rustc-env entries, and separate OUT_DIR values. Rustc target
artifacts are never linked into the Flux executable. Cargo check is a preparation
step, not evidence that Flux can compile that graph.

Metadata can retain optional dependency edges mentioned by weak feature
forwarding (`dependency?/feature`) even when Cargo does not build them. The
driver follows an optional edge only when the checked target's enabled features
activate that dependency. A proc macro used by a build script does not activate
an optional target dependency on that macro. Missing required target artifacts
and metadata/target feature differences still fail preparation.

Preparation writes `target/flux/cargo-plan.json`, or `--plan <file>`. Version 2
records source identities and dependency aliases, environments, cfgs, host
proc-macro artifact paths, build-script linker directives, and blockers. The
rustc version is recorded because host macro artifacts depend on its ABI. Cargo
may reuse cached build-script outputs and artifacts; preparation does not force
them to rerun. The plan contains explicit compile-time values and local paths.

Supported builds currently select one host binary on arm64 macOS. Function-like,
attribute and derive proc macros execute in the toolchain's separate
`rust-analyzer-proc-macro-srv` process. Cargo/rustc compile the host dylibs; Flux
checks and compiles their expanded target code. Imports, renamed exports, derive
helpers, cfg filtering, source locations, call-site and supported mixed-site
hygiene are retained. Macro-generated exports use the compiled host catalog.
Helper lists come from checked explicit source declarations; generated derives
without recoverable helper metadata reject ambiguous bare attributes before
attribute expansion, including builtin names that could be helpers. Such derives
work without these attributes; cfg, derive and qualified attributes keep their
ordinary handling. Each invocation crate has its own persistent host state.
Macro processes inherit the host environment, receive the invocation crate's
explicit overrides, and run from the workspace root. Flux's process environment
and current directory do not change.

The bridge accepts only the reviewed protocol 6 schema from rustc commit
`58cf8f9c35b355199727627d07ae34a4e2846d67` (1.99.1). Other commits or a missing
server component produce an error. This uses an unstable toolchain protocol,
not a portable Rust proc-macro ABI. Host export/helper declarations must appear
literally in their source root under the active cfg. Profile-dependent host export
declarations (`panic`/`debug_assertions`) are rejected because the helper metadata
reader uses target cfg rather than the host build profile. Generated declarations,
definition-site output hygiene and nested invisible delimiter groups remain
explicit errors. Input tokens retain their macro_rules contexts through span anchors. Macros writing to stdout
corrupt the server's protocol and fail; use stderr for logging. Host failures do
not restart a process with reset state. Expansions have a 60-second deadline,
a 32 MiB response limit, and token/depth limits.

Build-script native linker directives and distinct host/target script or macro
units also require further integration. Those are rejected rather than ignored.
There is no cross-compilation, unit-graph feature reconciliation, Cargo profile
mapping, rustc-compatible library artifact export, or `RUSTC=flux` adapter yet.

Explicit source environments contain common Cargo package/crate variables and
build-script outputs. Arbitrary process variables are not implicitly serialized
or inherited. Direct standalone `flux build file.rs` retains process environment
lookup. This avoids leaking unrelated process values into source-graph plans;
projects requiring other `env!` inputs need an explicit input interface extension.

This uses Cargo's documented [metadata and JSON messages](https://doc.rust-lang.org/cargo/reference/external-tools.html)
and [build-script outputs](https://doc.rust-lang.org/cargo/reference/build-scripts.html).
The expansion interface follows Rust’s [procedural-macro token model](https://doc.rust-lang.org/reference/procedural-macros.html).
Run `cargo test -p flux-cli --lib --test proc_macros --test cargo_host` for the
bridge, host lifecycle, real Serde expansion, and native comparisons.

The `registry_bitflags` integration test builds unchanged registry `bitflags`
2.13.2 with its `std` feature through this driver. Its consumer exercises exported
macros, flag operations, iteration, parsing, and formatting, and compares stdout,
stderr, and exit status with rustc in both Flux optimization modes. The test
checks registry source bytes and the consumer lockfile after compilation. The
pinned dev dependency supplies the registry fixture for offline test execution.
Run it with `cargo test -p flux-cli --test registry_bitflags` on arm64 macOS.
This covers the Cargo-backed driver; standalone registry resolution is still
unsupported.
