# Cargo, Flux and Keel

Research/design, 2026-10-23. The bounded C0 manifest driver is now
implemented as `flux package build/run`; see [the exact supported
subset](PACKAGE_DRIVER.md). A bounded local path source graph is also
implemented; general Cargo resolution, the compiler adapter and rustc backend
bridge remain proposals. Original experiments inspected Flux at
`04f2298844e761d793770c92a0f5c47dbf6eb1be` and its pinned Keel dependency
`1aa8c53686892e4e3c09aa0c512c03da02148f92`. Proposed commands and APIs below
remain proposals except for C0 and the bounded path-only C1 subset. Recorded
experiments used Cargo/rustc 1.99.0 on
arm64 macOS; compiler protocol details must be retested for other versions.

## Recommendation

Keep Keel as Flux's backend. Start with a thin manifest driver that asks
real Cargo for package information and invokes Flux for one executable.
Then add a source crate graph, retaining Cargo's resolver. Do not start by
reimplementing Cargo's package manager or rustc's metadata format.

If the immediate requirement is specifically `cargo build` driving Flux,
implement the restricted compiler adapter described under route A instead.
Its first milestone can also be a single dependency-free binary. That is
feasible without rustc library metadata, but it has more protocol work than
the manifest driver. Neither milestone builds this compiler workspace.

If the requirement is instead to compile ordinary Rust crates with Keel
while preserving today's Rust frontend and libraries, route B is the right
architecture. It is a separate backend project, not Flux self-hosting.

## Three different projects

### A. Real Cargo drives Flux as its compiler

```text
Cargo: manifests, resolution, features, scheduling, host tools, freshness
  -> flux-rustc: rustc command-line/probe/artifact protocol
     -> Flux syntax/HIR/types/MIR -> Keel IR/object -> linker
```

Cargo documents `RUSTC` as its compiler override. `RUSTC_WRAPPER` receives
the compiler path as its first argument, followed by compiler arguments.
`RUSTC_WORKSPACE_WRAPPER` applies to workspace packages and affects artifact
hashing. Both wrappers can nest. These hooks change executable invocation;
they do not provide a frontend, crate loader or metadata translator.
See [Cargo environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-reads).

Use a dedicated `flux-rustc` executable for `RUSTC`, rather than trying to
put `flux build` plus arguments into that variable. A wrapper variant must
explicitly consume the compiler-path argument; a replacement compiler does
not receive that extra argument. Replacing only workspace compilation
still leaves rustc-built dependencies that Flux cannot currently import.

Real Cargo can coordinate an alternative compiler whose artifacts are
private to that compiler. It does not follow that those artifacts are
interchangeable with rustc's. Initially reject library compilation and
`--extern`; eventually choose a Flux-only artifact protocol or implement
separate compilation with Flux metadata. Do not put an object into a file
named `.rlib` and claim rustc compatibility.

### B. `rustc_codegen_keel` retains rustc

```text
Cargo -> rustc frontend, macro expansion, types, borrow checking, metadata
         -> rustc monomorphized MIR/layout/ABI -> Keel -> objects
         -> rustc artifact and link machinery
```

Rust's documented dynamic backend entry point is a `dylib` exporting
`__rustc_codegen_backend`, returning a boxed `CodegenBackend`, loaded with
nightly `-Zcodegen-backend=PATH`.
See [codegen-backend](https://doc.rust-lang.org/unstable-book/compiler-flags/codegen-backend.html).
The [current backend trait](https://doc.rust-lang.org/nightly/nightly-rustc/rustc_codegen_ssa/traits/backend/trait.CodegenBackend.html)
includes crate code generation, joining work and hooks for metadata loading,
linking and target capabilities. The [backend guide](https://rustc-dev-guide.rust-lang.org/backend/backend-agnostic.html)
explains the shared `rustc_codegen_ssa` abstraction.

This needs a new lowering from **rustc** MIR/types/layouts into Keel IR;
`flux_codegen::lower_module` consumes **Flux** HIR/typed MIR and cannot be
plugged into that trait. Pin a nightly toolchain and compiler interfaces.
The [rustc_private requirements](https://doc.rust-lang.org/unstable-book/language-features/rustc-private.html)
include compiler development components; linking may also require LLVM
tools. This does not promise an LLVM-free toolchain.

Retaining rustc solves ownership of Rust language semantics and metadata.
Keel must still implement correct calling conventions, aggregates, layout,
statics, intrinsics, TLS, atomics and the selected panic strategy. Reusing
prebuilt standard libraries requires demonstrated ABI compatibility;
rebuilding libraries is another option, not proof of compatibility. Disable
unsupported LTO, unwinding and debug configurations explicitly. Proc macros
are host code and require a supported host build/load strategy as well.

A bounded B milestone would compile one `no_std` library function to an
object with the pinned backend and call it through a tested C ABI harness.
Next prove one Rust dependency pair, including a cross-crate generic, with
real rustc metadata. Existing Flux codegen success is not evidence that
this bridge exists. This route intentionally uses rustc for actual frontend
compilation and must be advertised that way.

### C. A standalone manifest/source-graph driver

```text
Cargo metadata JSON -> Flux build plan -> source crate graph
  -> Flux whole-program checking/monomorphization -> existing Keel path
```

This avoids rustc-compatible invocation and separate library artifacts.
It still needs actual crate boundaries: dependency aliases, identities,
editions, features, visibility, trait coherence and dependency scope.
Concatenating dependencies into modules changes Rust semantics, including
`crate::`, privacy, exported macros and two versions of the same package.

Use [cargo metadata, format version 1](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html)
for package IDs, source roots, target kinds, dependency kinds, aliases and
resolved features. Treat IDs as opaque; use `resolve.nodes[].deps`, not
just the deprecated flat dependency list. `--no-deps` omits the resolved
graph. `--filter-platform` filters resolution, while package declarations
still contain dependencies for other platforms.

Metadata is not a complete build-unit graph. In particular, package-level
features cannot describe every host/target and dependency-kind feature
instance. Cargo's [unit graph documentation](https://doc.rust-lang.org/cargo/reference/unstable.html#unit-graph)
states this limitation; that fuller interface remains nightly/unstable.
Do not design around old `--build-plan` examples: the same official page's
[build-plan section](https://doc.rust-lang.org/cargo/reference/unstable.html#build-plan)
records its removal in 1.93.0-nightly. Stable metadata is useful within a
declared restricted subset; it is not a stable export of all compiler jobs.

A complete Cargo replacement would additionally own registry/git fetching,
lockfiles, resolution, workspace inheritance, resolver versions, target
selection, configuration, profiles, build-script execution, jobserver,
incremental invalidation, tests, rustdoc and diagnostics. Avoid that scope.
A future `cargo-flux` custom subcommand can expose C without replacing
Cargo; [external tools](https://doc.rust-lang.org/cargo/reference/external-tools.html)
documents that extension point and Cargo's JSON build messages.

## What exists locally

- [CLI](../crates/flux-cli/src/main.rs): `build FILE [-o OUTPUT] [-O]`,
  `run FILE`, and restricted `package build/run --manifest-path PATH [--bin NAME]`;
  no compiler probes or `--extern`.
  `check` currently checks syntax only, so it cannot implement `cargo check`.
- [Build pipeline](../crates/flux-cli/src/build.rs): loads a root/module
  tree, checks HIR/types/main/MIR, lowers the program, compiles one object
  and links one executable. A reusable `CompileRequest` accepts an explicit
  crate name; direct file builds use the file stem, package builds the target name.
- [Source database](../crates/flux-hir/src/source.rs) embeds Flux's own
  `core`, `alloc` and a bounded `std` facade. [Item collection](../crates/flux-hir/src/item_tree.rs)
  has `CrateId`, a crate list and private `add_crate`, but public lowering
  accepts one program root. An arbitrary external crate is rejected.
  This is a starting point for C, not an existing Cargo crate loader.
- [Configuration](../crates/flux-hir/src/cfg.rs) uses compiler-host
  constants, fixed debug assertions/panic settings, and treats user feature
  predicates as false. It needs an explicit per-compilation configuration
  before honoring Cargo features, profiles or cross targets.
- [Flux codegen](../crates/flux-codegen/src/lib.rs) already calls
  `keel_ir::verify_module`, `keel_opt::optimize_with_tier`,
  `keel_codegen::compile_program` and the Keel assembler. `compile_object`
  returns bytes for arm64 Mach-O or x86-64 ELF. `link_executable` currently
  supports arm64 macOS only, using the SDK's `libSystem.tbd` and Keel's
  signed executable writer; Linux linking explicitly errors. Keel target
  availability does not imply Flux executable support on that target.
- [Lowering](../crates/flux-codegen/src/lower.rs) owns Flux layouts,
  monomorphization, symbols and the C `main` wrapper. Legacy-style mangled
  symbol spelling alone is not Rust ABI interoperability.
- [Workspace manifest](../Cargo.toml) pins six Keel crates to the commit
  above. Their local checkout exposes `Target::{host,triple,from_triple}`,
  `compile_program(&Module, Target)` and Keel link `Input`/`Options` APIs.
  It does not expose a rustc frontend or Cargo integration adapter.
  Keel's `keel-codegen/build.rs` generates `patterns.rs`, consumed by
  `src/select.rs` using `include!(concat!(env!("OUT_DIR"), ...))`.
- [Self-hosting](SELF_HOSTING.md) records the broader blockers: real
  dependencies, unsafe code, generated source, proc macros and missing
  library/runtime APIs. [Contributing](../CONTRIBUTING.md) supplies the
  current arm64 runtime comparison workflow.

## Contracts that orchestration cannot supply

**Rust dependencies and ABI.** Rustc libraries contain compiler-specific
metadata describing items, types, traits, macros, identities and sometimes
generic MIR. An `.rmeta` is not linkable code. The
[metadata guide](https://rustc-dev-guide.rust-lang.org/backend/libs-and-metadata.html)
also documents version checks and metadata inside `.rlib` archives.
`--extern alias=PATH` identifies a direct crate; it is not merely a native
linker option. The [rustc command-line reference](https://doc.rust-lang.org/rustc/command-line-arguments.html#--extern-specify-where-an-external-library-is-located)
documents this distinction and the `-L` search path for indirect crates.
Changing Cargo cannot make Flux read those formats, monomorphize rustc MIR
or call arbitrary Rust symbols correctly. A deliberately tested C FFI
boundary remains possible; it is not a general Rust dependency bridge.

**Build scripts.** Cargo compiles and runs `build.rs`, supplies `OUT_DIR`,
and consumes its ordered instructions, including native linking, cfg,
environment and rerun rules. Generated files persist between script runs;
scripts must not assume an empty output directory. See
[build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html).
A can let Cargo orchestrate them only once its compiler can compile the
host script and dependencies. C would have to implement their protocol.
Using rustc to build them must be an explicit mixed-toolchain mode, never
a silent fallback. Reject scripts in the first subset.

**Proc macros.** These are compiler-loaded host libraries that run at
compile time and transform token streams, including derives and attribute
macros. See the [Rust Reference](https://doc.rust-lang.org/reference/procedural-macros.html).
Flux's built-in derives and declarative macros do not establish a
`proc_macro` bridge. Building a dylib and finding its filename are only a
small part of the required execution/bridge protocol. Reject proc macros
initially; compiler CLI derives such as clap remain a self-hosting blocker.

**Standard library.** Flux's embedded `core`/`alloc` are a supported subset,
not rustup's sysroot. They do not provide a complete `std`, filesystem,
process, synchronization or unwinding runtime. Cargo's unstable
[build-std](https://doc.rust-lang.org/cargo/reference/unstable.html#build-std)
can request library source builds; it cannot add missing language support.

## Offline protocol experiments

All projects and artifacts were temporary and outside this worktree.
Network access was disabled at the Cargo command level with `--offline`.
Each subprocess had a 10–60 second timeout. The protocol recorder was a
transparent wrapper around **real rustc**, used solely to observe Cargo;
its successful compilations are not Flux compatibility results.

Toolchain: Cargo `1.99.0 (5f94df478 2026-08-27)`, rustc
`1.99.0 (b940084d7 2026-09-28)`, `aarch64-apple-darwin`.

1. A dependency-free edition-2024 binary printed `probe=42` and exited 0.
   Its dev profile set `panic="abort"`, `debug=0`, `incremental=false`.
   `cargo build --offline -vv --message-format=json` succeeded through the
   rustc recorder. Before compiling, Cargo sent `-vV` and this combined
   probe (line breaks added):

   ```text
   - --crate-name ___ --print=file-names
   --crate-type bin --crate-type rlib --crate-type dylib
   --crate-type cdylib --crate-type staticlib --crate-type proc-macro
   --print=sysroot --print=split-debuginfo --print=crate-name
   --print=cfg -Wwarnings
   ```

   The real compiler returned six artifact names, a sysroot, the three
   supported split-debug modes, `___` as a delimiter, and cfg entries.
   [Print options](https://doc.rust-lang.org/rustc/command-line-arguments/print-options.html)
   documents ordered queries. This probe does **not** require building all
   six kinds, but an adapter must explain unsupported kinds truthfully.

2. The binary compilation used this argument shape (hashes normalized):

   ```text
   --crate-name probe_bin --edition=2024 src/main.rs
   --error-format=json --json=diagnostic-rendered-ansi,artifacts,future-incompat
   --crate-type bin --emit=dep-info,link
   -C panic=abort -C embed-bitcode=no
   --check-cfg 'cfg(docsrs,test)' --check-cfg 'cfg(feature, values())'
   -C metadata=HASH -C extra-filename=-HASH
   --out-dir TEMP/bin-target/debug/deps
   -C strip=debuginfo -L dependency=TEMP/bin-target/debug/deps
   ```

   Cargo published `debug/probe-bin` from the compiler output. A second
   build emitted `compiler-artifact` with `fresh:true` and made no source
   compilation call (probes still ran with compiler-info caching disabled).

3. A second fixture added a renamed path library, an identity proc macro
   with no dependencies, and `build.rs` generating a constant plus a
   `cargo::rustc-env` instruction. Four source compilation calls built the
   library, proc macro, build script and binary. The binary got
   `--extern renamed=...rlib`, `--extern probe_mac=...dylib`, `OUT_DIR`,
   and the generated environment variable. It printed `generated:43`,
   exit 0. Cargo JSON included `build-script-executed`.

4. `cargo check` on that fixture used `--emit=dep-info,metadata` and passed
   the normal library as `.rmeta`; the proc macro remained a loadable
   `.dylib`. A check still needs executable host tooling. This observation
   agrees with [Rust linkage](https://doc.rust-lang.org/reference/linkage.html).

5. Full offline `cargo metadata --format-version 1` returned three
   packages and preserved dependency alias `renamed`. It invoked compiler
   discovery and the combined probe despite compiling no source.
   `--filter-platform aarch64-apple-darwin` added a target-specific probe.
   With `RUSTC` set to a script that only exits 86, full metadata failed
   at `-vV`, Cargo exit 101. `metadata --no-deps` succeeded even with that
   rejecting compiler and returned no resolve graph. Therefore a metadata
   driver cannot generally assume it needs no installed compiler: explicitly
   retain rustc for **discovery only**, or supply Flux discovery first.

6. `cargo build --offline -p flux-cli --target-dir TEMP/flux-target`
   built the current Flux with rustc. That Flux directly built the first
   fixture through Keel; its executable printed `probe=42`, exit 0.
   Setting `RUSTC=TEMP/flux-target/debug/flux` for real Cargo failed at
   `flux -vV`, Flux exit 2 / Cargo exit 101:

   ```text
   error: unexpected argument '-v' found
   Usage: flux <COMMAND>
   ```

For compiler identity, current
[Cargo source](https://doc.rust-lang.org/nightly/nightly-rustc/src/cargo/util/rustc.rs.html)
extracts `host:` and parses `release:` as semver. The
[target-info parser](https://doc.rust-lang.org/nightly/nightly-rustc/src/cargo/compiler/build_context/target_info.rs.html)
documents the combined query, delimiter, and unsupported-crate diagnostic
recognition. These are implementation observations, not a versioned plugin
ABI. The browsed nightly sources can differ from the measured stable build.

### Repeating the smallest protocol probe

This self-contained script compiles only a tiny real-rustc program and
records argv. It writes no repository files and needs no downloaded crates.
Run from this checkout to select its stable toolchain. Extend its temporary
manifest with local dependencies/build.rs to repeat the graph observations.

```sh
python3 - <<'PY'
import json, os, pathlib, subprocess, sys, tempfile
p = pathlib.Path(tempfile.mkdtemp(prefix="flux-cargo-probe-"))
(p / "src").mkdir()
(p / "Cargo.toml").write_text('''[package]
name = "probe-bin"
version = "0.1.0"
edition = "2024"
[profile.dev]
panic = "abort"
debug = 0
incremental = false
''')
(p / "src/main.rs").write_text('fn main() { println!("probe={}", 42); }\n')
w = p / "record.py"
w.write_text("#!" + sys.executable + '\n' + '''import json, os, sys
with open(os.environ["PROBE_LOG"], "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\\n")
os.execv(sys.argv[1], sys.argv[1:])
''')
w.chmod(0o755)
env = os.environ.copy()
for key in ("RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
            "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_BUILD_DIR"):
    env.pop(key, None)
env.update(RUSTC_WRAPPER=str(w), RUSTC_WORKSPACE_WRAPPER="",
           CARGO_TARGET_DIR=str(p / "target"), CARGO_CACHE_RUSTC_INFO="0",
           CARGO_INCREMENTAL="0", PROBE_LOG=str(p / "calls.jsonl"))
for _ in range(2):
    subprocess.run(["cargo", "build", "--offline", "-vv",
                    "--message-format=json", "--manifest-path",
                    str(p / "Cargo.toml")], env=env, check=True, timeout=60)
print("Recorded protocol:", p / "calls.jsonl")
print((p / "calls.jsonl").read_text())
PY
```

## Staged implementation and acceptance gates

### C0: implemented bounded manifest-driven executable

This was the original dependency-free boundary; C1 below now extends it
with local source libraries. The following describes the C0 milestone.

`flux package build --manifest-path PATH [--bin NAME]` and
`flux package run ... -- PROGRAM_ARGS` are implemented. The exact declaration,
configuration and environment allowlist is in [PACKAGE_DRIVER.md](PACKAGE_DRIVER.md).
The driver accepts one package with
one selected binary, edition 2024, no dependency declarations, features,
build scripts, library target or proc macros; only arm64 macOS. Reject
other targets and custom profiles/options rather than ignoring them.
Use `cargo metadata --offline --no-deps --format-version 1`; inspect the
selected package's declarations and discovered targets before compilation.
No resolve graph is needed for this milestone. Cargo does manifest
interpretation; Flux compiles the selected source, including its modules.

The implementation extracts a minimal `CompileRequest` for the existing
fixed edition-2024 host configuration, explicit crate name and output.
It rejects other configurations instead of applying Cargo profiles. The
following design checklist retains future generalized configuration/API
work, which is not claimed by the implemented subset:

- Add a manifest-plan module in `flux-cli` (or a small `flux-build` crate
  once shared). Select by package/target identity and `src_path`, not by
  assuming `src/main.rs`. Reject ambiguity and unsupported declarations.
- Extract current build logic into a reusable typed `CompileRequest` with
  root path, explicit crate name, edition, target, configuration, mode and
  output paths; return diagnostics, loaded files and produced artifacts.
  Preserve `flux build FILE` as a convenience caller.
- Add `CompilationConfig` and thread it into HIR/cfg evaluation. Validate
  requested settings against what is implemented; do not silently inherit
  arbitrary Cargo release/debug behavior. Scope cached library state by
  relevant target/configuration when it can affect embedded libraries.
- Emit under a distinct `target/flux/` namespace. Initially rebuild every
  time; correctness is more useful than premature Cargo-cache emulation.

Acceptance: offline build/run of the recorded binary produces `probe=42`,
exit 0, using Flux/Keel; a custom binary name/path and file module also
work. No rustc source compilation occurs after bootstrapping Flux. Invalid
programs fail with Flux diagnostics. Library/build-script/dependency/feature
inputs and unsupported targets fail explicitly before compilation. This
is manifest support, not Cargo compiler compatibility.

### C1: source dependencies, whole program

Implemented subset: edition-2024 and bounded edition-2021 local path libraries, including transitive
edges, aliases, shared crate identities and privacy. Sources compile together
without `.rlib` artifacts. Exported declarative dependency macros preserve
`$crate` identity through aliases/reexports. Registry/git packages, workspaces,
features and build scripts remain unsupported. See
[the package driver boundary](PACKAGE_DRIVER.md). The broader design below
remains planned.


Add an explicit graph API, for example
`lower_graph(db, &[CrateInput], entry_crate)`, where `CrateInput` carries a
stable internal ID, package identity, root, edition, cfg set and a map of
extern aliases to crate IDs. Adapt the existing crate list/collection and
name resolution so dependencies see only their declared predecessors.
Extend type checking and monomorphization to dependency bodies; their
public generic bodies must remain available. Preserve privacy, coherence,
macro scope and duplicate-package identities. Never rewrite source into
one synthetic module. Include crate identity in generated symbols, rather
than only the current name-based path, so distinct versions do not collide.
Check `main` only in the entry crate. All program instances go through Keel
together.

Start with an acyclic graph of local path libraries, no build/dev/proc-macro
dependencies, optional dependencies, target predicates or differing feature
instances. For broader Cargo graphs, query full metadata with an **explicit** discovery compiler;
on the measured toolchain rustc probes are sufficient and no source is
compiled. Fail if any required unit/configuration is outside the subset.
Later replace those probes with Flux's A discovery implementation, whose
cfg must agree with actual Flux compilation. Reusing rustc cfg indefinitely
would select unsupported branches incorrectly.

Acceptance: binary -> renamed path library -> second library; cross-crate
generic execution; dependency `crate::` paths; privacy rejection; aliases
and same-named packages cannot leak into unrelated crate scopes. Compare
observable output/exit status against rustc. Check disabled/unsupported
dependency kinds and configurations are rejected, not omitted. This stage
avoids serialized metadata but rebuilds/rechecks the source closure.

### A1: optional real-Cargo binary adapter

Proposed future invocation, after implementation:

```sh
RUSTC=/absolute/path/flux-rustc RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= \
  CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/flux-cargo \
  cargo build --offline --bin probe-bin
```

Implement discovery before compilation. Support `-vV` with honest Flux
identity, semver release and host fields. Decide and document how Cargo's
Rust-version checks map to this alternative compiler; do not fabricate
rustc's version/commit or claim general language support to pass a probe.
Test Cargo's version-dependent flag selection against the chosen identity.
Provide a Flux-owned sysroot description, ordered `--print` responses and
cfg values from the same configuration used by HIR. Initially report only
binary support: the combined query must return the binary filename and
recognizable unsupported-crate warnings for other types, not fake library
support. Verify the complete query against the selected Cargo release.

Parse both `--flag=value` and split forms. Map crate name, edition, input,
output directory/suffix, emit modes and supported profile options to
`CompileRequest`. Honor configuration/check-cfg requests or fail explicitly.
Define a small supported profile: panic abort, optimization off or full,
no incremental, no debug information, no LTO; wire overflow/debug assertions
consistently before claiming those settings. Semantics-neutral options
such as disabling embedded LLVM bitcode can be accepted explicitly.
Unknown flags and `--extern` fail. A successful probe is not permission to
ignore later flags. Library/test/rustdoc/check support are separate gates.

Produce the exact hashed executable and Makefile dep-info naming requested
by Cargo. Track every loaded module; include embedded-library/compiler
identity in invalidation. Send rustc-shaped diagnostic/artifact JSON to
stderr when requested; Cargo's JSON output is a different stream.
See [rustc JSON](https://doc.rust-lang.org/rustc/json.html) and
[Cargo build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html).
Use artifact messages/output parameters rather than assuming Cargo's
internal directory layout is stable.

Acceptance: real offline Cargo builds and runs the dependency-free fixture
with Flux/Keel, then reports it fresh; editing any module rebuilds it;
syntax/type errors fail in Cargo JSON; two named binaries have correct
artifact paths. A source-compilation invocation must never execute rustc.
Unsupported flags, libraries, build scripts and proc macros must fail
clearly. Do not claim `cargo check` until it performs semantic checking and
has an explicitly defined metadata output contract; empty `.rmeta` files
are not an implementation. Keep rustc artifacts out of the Flux cache.

### Later gates

Choose **one** dependency artifact strategy for A: serialized Flux
interfaces plus generic bodies for separate compilation, or a documented
Flux source-descriptor artifact that lets later jobs reload the graph.
The latter can cooperate with Cargo scheduling but is whole-program
compilation with private artifacts, not rustc `.rlib` interoperability.
It also requires sound dep-info/invalidation and no fake check results.
C's source graph is reusable here.

Add features/configuration and per-unit identities before broadening
dependency support; metadata alone is insufficient for unrestricted Cargo
resolver behavior. Add host tools, generated source and proc-macro execution
as separate milestones with runtime comparisons. Expand Flux's language and
runtime against the actual dependencies in SELF_HOSTING.md. A complete
compiler build requires all those capabilities regardless of driver choice.

## Scope and validation of this document

The original research changed only this document. Read AGENTS.md, CONTRIBUTING.md, SELF_HOSTING.md,
workspace/CLI/codegen manifests, CLI/build/HIR/codegen and the pinned local
Keel API. Browsed primary Rust/Cargo documentation linked above. Ran the
bounded offline Cargo protocol experiments, built the current CLI offline,
and directly compiled/executed the tiny program with Flux/Keel. The subsequent
C0 implementation has eight focused offline tests, including rejecting
rustc/wrapper scripts that record zero invocations. No rustc-compatible adapter,
dependency crate loader, backend bridge or Cargo replacement is implemented; no
workspace self-host or general Cargo compatibility is established. Full
`just check` is unnecessary for this documentation-only change.
