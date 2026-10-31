# flux-cli

`flux` manages packages. `fluxc` compiles individual source files. Both binaries
share the compiler library. Install them together: `flux` invokes its sibling
`fluxc` for package operations so artifact identities stay consistent.
Package builds use native planning by default;
`--cargo` explicitly selects the Cargo-backed host preparation route.

The `flux` package manager and `fluxc` compiler for [Flux](../../README.md),
an experimental compiler for a subset of Rust.

`fluxc print host` and `fluxc print cfg` report Flux’s native target and baseline
configuration without querying rustc. Resolution, planning and packages without
host tools do not require rustc. Host preparation still uses rustc.

```sh
fluxc lex file.rs      # tokens, trivia included
fluxc parse file.rs    # syntax tree and errors
fluxc check a.rs b.rs  # errors only; exits with 1 if there are any
```

## Native lockfile generation

```sh
flux generate-lockfile --manifest-path Cargo.toml
flux fetch --locked
flux build --offline -p flux-cli --bin fluxc -O
```

`generate-lockfile` resolves every package in the workspace and writes Cargo.lock
version 4 without invoking Cargo. It reads crates.io sparse-index metadata,
local manifests and git checkouts. It does not download registry archives or run
build scripts. Like Cargo's command, it reselects versions for the complete graph;
it does not preserve existing versions merely because they were locked.

Resolution backtracks across compatible version choices, unifies semver-compatible
releases, permits incompatible versions together, validates requested features,
and rejects dependency cycles and conflicting `links` declarations. Yanked
registry releases are excluded even if a previous lockfile used them. The lockfile
pass includes workspace development dependencies, all target conditions and all
workspace features, including implicit path members. Weak feature forwarding can
retain a lockfile edge without making that crate active in native build planning.
Edition 2024 workspace dependencies can override inherited default features.

Resolver 3 prefers Rust-version-compatible releases, using the lowest declared
workspace `rust-version`. Without a declared workspace Rust version, no MSRV
ceiling is inferred from the installed toolchain.
If no matching compatible release exists it can fall back to a newer requirement.
Resolver 1/2 defaults to allowing newer Rust requirements. Cargo configuration's
`resolver.incompatible-rust-versions` policy and `--ignore-rust-version` can change
this preference. Version choices can differ from Cargo for loose requirements
because the search order and workspace Rust-version heuristic are not identical.

Git dependencies support full commit IDs, advertised revisions, branches, tags
and the default branch. Branches and annotated tags are resolved to commit IDs;
lockfiles record the precise commits. Offline branch/tag resolution requires a
reference cached by a previous native resolution. Unadvertised short commit IDs
and arbitrary git revision expressions are not supported yet.

Sparse-index responses are validated and cached under `CARGO_HOME`. Each online
run refreshes the records it needs. `--index-url <URL>` selects a sparse mirror
with the same crates.io identities, including local file mirrors for testing.
`--offline` uses cached index records and considers only downloaded registry
releases. For the default endpoint it can also read Cargo's version 3 sparse-cache
format. Locked fetching alone does not populate an index cache.

Resolution completes before the lockfile is replaced by rename. Failure leaves
an existing lockfile unchanged. Semantically unchanged lockfiles retain their
original bytes. `--locked` checks the newly resolved graph without changing the
file and fails if it differs. A checksum change for an already locked registry
version also fails. Search is bounded to 100,000 decisions and a depth of 512.

Alternate registries, patch/replace overrides, workspace globs, artifact
dependencies and package-specific or precise updates are not supported yet.
Use `generate-lockfile`, then `fetch --locked`; automatic resolution during
`fetch` or native builds is not enabled.

Resolution has been checked against Cargo with ten regression cases and a fresh
98-package workspace graph on arm64 macOS and x86-64 Linux. An empty-cache macOS
run resolved from the public index, fetched the selected crate and built its
program with Cargo blocked. A separate snapshot built the updated compiler with
Flux; that generated compiler then resolved a graph requiring backtracking,
fetched it and built and ran it offline with Cargo blocked. The complete macOS
suite passed 478 targets. Both hosts passed workspace Clippy. Local evidence is
indexed by `out/lockfile-resolution-audit/proof.json`.

## Locked dependency fetching

```sh
flux fetch --locked --manifest-path Cargo.toml
flux fetch --locked --offline
flux build --offline -p flux-cli --bin fluxc -O
```

Fetching reads an existing Cargo.lock version 3 or 4 and downloads every locked
registry or git source, including optional dependencies and other platforms.
It does not select versions, query a registry index, run Cargo or rustc, or
change the lockfile. Path dependencies remain in their existing locations.
Native planning and builds stay offline; fetching is an explicit separate step.

Registry downloads use the crates.io download API and must match the lockfile's
SHA-256 checksum. Extraction checks tar header checksums and package identity,
rejects paths outside the package root, links, duplicate files and special
entries, and installs the completed tree by rename. Supported tar entries are
regular files, directories, GNU long paths and local PAX path/size records.
Archives are limited to 128 MiB compressed, 512 MiB uncompressed and 100,000
entries. Executable file bits are retained without privileged mode bits.
Temporary downloads and extraction trees are removed on failure. A verified
archive may remain cached if extraction fails.

Git sources use the exact 40-digit locked commit, not the current branch tip.
Submodules are fetched through Git using its normal transport policy. Sources
live under `CARGO_HOME` (default `~/.cargo`) beside existing Cargo cache entries.
Existing extracted trees and git checkouts retain the planner's trusted-cache
contract; fetching does not rehash every existing source file. A corrupt cached
archive fails checksum verification instead of being installed.

`--offline` reuses extracted sources, existing git checkouts or verified cached
registry archives. It can extract an archive without a download. Missing sources
fail without network access. Fetching uses `curl`, `gzip` and `git` from PATH as
needed; curl configuration files are disabled, and the default HTTPS endpoint
only follows HTTPS redirects. Git retains the user's credential helpers.

`--download-base <URL>` selects a mirror of the crates.io download API, appending
`/<name>/<version>/download`. HTTPS, HTTP and local file mirrors are accepted;
package identities and checksums still come from Cargo.lock. Alternate registry
identities, source replacement and patch/replace overrides are not supported yet.
Use `generate-lockfile` above to create or regenerate a crates.io lockfile.

Empty-cache fetching has been verified on arm64 macOS and x86-64 Linux for all
85 locked source packages in this repository. Both runs left Cargo.lock unchanged
with Cargo blocked, then replayed fetching and planning offline. The macOS run
built the optimized compiler from that fetched cache. The generated compiler
then fetched a registry crate and a pinned git dependency into another empty
cache, and built and ran their program offline with Cargo blocked. The complete
macOS suite passed 477 targets; both hosts passed the seven fetching regression
tests and workspace Clippy. Local evidence is indexed by
`out/locked-fetch-audit/proof.json`.

## Native offline planning

```sh
flux plan --manifest-path Cargo.toml -p flux-cli --bin fluxc
```

This command reads manifests and Cargo.lock directly. It never invokes Cargo,
accesses the network, runs build scripts, or loads proc macros. Dependencies must
already exist in `CARGO_HOME` (default `~/.cargo`); `flux fetch --locked`
can populate it. Flux supplies its baseline host triple
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

Planning does not execute host tools. Use `prepare` or `build` to prepare or
compile the graph without Cargo, as described below.
Alternate registries, source replacement, patch/replace overrides,
workspace member globs, resolver 1, and cross compilation are not supported.
Unsupported source/target overrides in Cargo configuration fail explicitly.

## Native source builds

```sh
flux prepare --manifest-path Cargo.toml -p flux-cli --bin fluxc
flux build --manifest-path path/to/Cargo.toml -p application -O
```

These commands use native offline planning. They compile host libraries,
proc-macro dylibs, and build-script executables with direct rustc calls. They run
build scripts from their package directories, then Flux compiles target source
crates and generated includes. Rustc does not check or compile target libraries
or binaries. Host and target units retain separate features and `OUT_DIR`s.
The existing toolchain-matched proc-macro server executes host dylibs.

Build scripts receive package, feature, target cfg, and profile variables, plus
`DEP_*` metadata from direct dependencies with `links` declarations. Both
`cargo:` and `cargo::` output prefixes support cfg, environment, linker search
paths and libraries, and links metadata. Build-script environment names currently
require ASCII. Unknown modern directives and custom
linker arguments fail explicitly. Native linker directives can prepare rustc
host tools; Flux target linking still rejects them.

Each invocation recompiles host units and reruns build scripts. Host tools use
optimization level 0 with debug assertions; `-O` selects Flux target optimization.
Cargo profiles and rustc wrappers are not interpreted. Existing generated output
is not reused. Artifacts and per-script stdout/stderr live under
`target/flux/native-host/<run>/`. Preparation writes a separate version 1
`flux-native-prepared` report to `target/flux/native-prepared-plan.json`, or
`--plan`. The report records rustc identity, the native graph, host artifact
paths, generated directories, and parsed script output. It is an audit report,
not a replay input. A failed script stops preparation and includes its output
in the diagnostic.

The driver never invokes Cargo. Build scripts may invoke their own external
tools; scripts that explicitly call Cargo still require it. Native preparation
retains the planner's source and workspace limits. It currently uses rustc
for proc macros with dependencies. The standalone `flux package` driver also
offers Flux compilation for dependency-free macro hosts.

On arm64 macOS, native preparation has built all 31 host artifacts and run all
13 build-script units for Flux's compiler graph, including five proc macros.
The complete optimized compiler build passed with a failing Cargo trap and
unchanged guarded repository sources. The generated compiler passed help/version
checks and six native comparisons against rustc, then built and ran a separate
proc-macro graph through its own native driver with Cargo blocked.

On x86-64 Linux, the native route built the optimized compiler in 288.6 seconds
from production sources at `1345009`, with a test-only PATH fix allowing Bash
for the system linker wrapper. The graph contained 85 packages and 102 build
units, with 30 host artifacts, 12 build-script units and five proc macros.
Cargo stayed blocked throughout the compiler build. All 5,713 guarded repository
and cached dependency files stayed unchanged through final validation.
The generated compiler passed help/version checks and ten native comparisons
against rustc: pointer array APIs, bootstrap collection APIs, BTreeMap,
x86 intrinsics and x86 crypto intrinsics, each in both output modes. It also
built and ran a separate proc-macro graph through its own native driver with
Cargo blocked. Formatting, workspace Clippy, 43 runner tests, all 476 Linux test
targets and workspace/example source checks passed. Local evidence is indexed
by `out/native-linux-validation/proof.json`.

The native route has not yet been verified through a stage 2 rebuild.

## Cargo-backed source builds

```sh
flux prepare --cargo --manifest-path path/to/Cargo.toml -p application --offline
flux build --cargo --manifest-path path/to/Cargo.toml -p application --offline
flux build --cargo --manifest-path path/to/Cargo.toml -p application --offline -O
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

Supported builds select one host binary on arm64 macOS or x86-64 Linux. Function-like,
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
or inherited. Direct standalone `fluxc build file.rs` retains process environment
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
