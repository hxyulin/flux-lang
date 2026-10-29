# Full Flux compiler source audit

Run the guarded audit on arm64 macOS:

```sh
cargo test -p flux-cli --test flux_compiler_readiness -- --ignored --nocapture
```

For a temporary backend experiment, apply Cargo path patches for all seven Keel
crates in the ignored `.cargo/config.toml`, resolve the local lockfile, and set
`FLUX_COMPILER_KEEL_CHECKOUT` to that checkout's absolute path. The checkout's
HEAD must equal the reviewed git pin. The audit then verifies each Keel package's
exact local manifest path while preserving all feature and dependency-edge checks.
It saves `local-keel-checkout.json` and `local-keel.patch`, and guards the actual
backend source bytes and Cargo patch configuration. This mode proves only the
recorded local backend; it does not validate the unmodified pinned revision.
Keep the original lockfile when setting up the experiment so it can be restored
with the local Cargo configuration removed afterward.

The outer audit is bounded to 20 minutes and its Flux worker to 18 minutes,
including frontend checks, both object builds and native compiler smoke tests.

The current graph uses git-pinned Keel `d17902eb`, without local path patches.
It contains 87 packages after adding the backend's bbtree/Verus closure and
splitting `flux-ty-ir` and `flux-solve` out of `flux-ty`, plus the CLI-only
`flux-proc-macro` host token API. HIR and type-checker source graphs keep
their existing dependencies. The
recorded stage-1 results later in this document use the prior patched backend;
they do not establish self-hosting of the new pin. Its complete backend audit
initially stopped during discovery of the host-generated `Structural` proc
macro. Generated export catalogs now load; the backend rerun reaches four syntax
and 339 HIR diagnostics in `out/keel-backend-readiness/31393`. No new complete
compiler native claim follows from that admission-only result.

`graph.json` records the locked package versions, sources, declared editions,
resolved features and normal dependency edges. It contains 87 packages, including
host-only dependencies of procedural macros. The target closure is recorded by the resolved graph. Each ordinary target library is loaded from its unchanged source;
procedural macro crates are represented by their actual rustc-built host artifacts.
The unchanged `flux-cli/src/main.rs` is a separate final source root, with the
implicit dependency on the package's `flux_cli` library modeled explicitly.

Cargo and rustc provide host build-script outputs, procedural macros and a
reference compiler. Cargo's actual JSON build messages, generated output sources,
macro artifacts and configuration are retained. The reference compiler must run
`--help`, `--version`, compile the smoke program and execute the result. Host
invocations use the same `argv[0]` (`flux`) for both compiler implementations;
stdout and stderr are compared byte-for-byte, without text normalization.

Flux proceeds through syntax, HIR, typing and complete MIR only when each earlier
stage is clean. It attempts object generation and native execution only after
all frontend diagnostics are zero. Both optimization modes must run the CLI
contracts and compile and execute the smoke program before a native milestone
can be claimed. Source files, manifests, lockfile, embedded libraries, harness,
configuration, generated sources and host macro artifacts are guarded against
changes during the run. Diagnostics report physical paths and source ranges.

The initial audit in `out/flux-compiler-readiness/62885` recorded:

- 3,329 guarded files, 50,831,137 bytes, with both final byte guards clean.
- Successful rustc-built real CLI contracts and smoke output `compiler smoke 34`.
- Zero syntax diagnostics and 182 HIR diagnostics. Types, MIR and codegen did not run.

The follow-up in `out/flux-compiler-readiness/62856` completed in 99.68 seconds:

- 3,368 captured files, with both byte guards clean.
- Zero syntax and HIR diagnostics.
- 220 type diagnostics. MIR and codegen did not run.

The next guarded run, `out/flux-compiler-readiness/14771`, completed in 104.88
seconds with zero syntax/HIR diagnostics and 104 type diagnostics. Both byte
guards were clean; MIR and codegen did not run.

The guarded `out/flux-compiler-readiness/27591` run completed in 108.56 seconds
with zero syntax/HIR diagnostics and 75 type diagnostics. Both byte guards were
clean. MIR and codegen remain gated on clean types.

The guarded `out/flux-compiler-readiness/53864` run completed in 105.95 seconds
with zero syntax/HIR diagnostics and 24 type diagnostics. Both byte guards were
clean. The remaining report contains one clap lookup ambiguity and TOML parser
combinator inference/projection diagnostics; MIR and codegen did not run.

The guarded `out/flux-compiler-readiness/63255` run completed in 205.06 seconds
with zero syntax, HIR and type diagnostics, then 306 full-MIR diagnostics.
Both byte guards were clean across 3,421 captured files. The MIR inventory
contains recursive trait obligations, opaque/caller/declaration lifetime proofs,
iterator projection rows, callback provenance and constant promotion. Codegen
did not run.

The guarded `out/flux-compiler-readiness/1363` run completed in 215.33 seconds
with zero syntax, HIR and type diagnostics, then 84 full-MIR diagnostics.
Both byte guards were clean across 3,441 captured files. The remaining report
has 59 impl-header instantiation errors, 16 declaration-bound errors, four
closure-local type-outlives requirements, three caller-bound errors, one method
signature mismatch and one temporary borrow. Codegen did not run.

The guarded `out/flux-compiler-readiness/24651` run completed in 253.58 seconds
with zero syntax, HIR and type diagnostics, then 17 full-MIR diagnostics.
All 17 are impl-method signature comparisons in winnow. Both byte guards were
clean across 3,449 captured files. Codegen did not run.

The later guarded `out/flux-compiler-readiness/56486` run completed in 213.38
seconds with zero syntax, HIR and type diagnostics, then 500 MIR diagnostics.
There are 498 callback-generality errors in toml_edit and two missing
associated-equality input occurrences in serde_json. The prior winnow errors no
longer appear. Both byte guards were clean across 3,454 files. Codegen did not run.

The guarded `out/flux-compiler-readiness/46721` run completed in 220.41 seconds
with zero syntax, HIR and type diagnostics, then 53 callback-generality MIR
diagnostics in toml_edit. Both byte guards were clean across 3,459 files. The
Serde projection failures are gone, and the universal-table panic found by an
intermediate diagnostic run no longer occurs. Codegen did not run.

The guarded `out/flux-compiler-readiness/61310` run completed in 532.90 seconds
with zero syntax, HIR, type and MIR diagnostics. It reached unoptimized codegen
and stopped at zmij's SIMD inline assembly (`lib.rs`, bytes 33717..33766): the
pinned backend rejects vector operands and the AArch64 `v` modifier. All three
byte guards were clean across 3,461 files. No compiler object or executable was
produced. Lowering diagnostics are now recorded separately by subsequent runs.

The guarded local-backend run `out/flux-compiler-readiness/665` passed in
379.15 seconds. All four frontend reports and both lowering reports were empty.
Both Flux-built CLI executables passed help/version checks, compiled the smoke
program and executed it with stdout, stderr and status matching the rustc-built
reference. Source, parent and pre-codegen integrity reports were empty. This
uses the temporary Keel vector-constraint/`v`-modifier patch based on `f37f0bd`,
recorded in the artifact directory. It establishes a local-backend stage-1 CLI;
no complete stage-2 compiler rebuild has been tested. A follow-up vector
regression passed all four stage-1 producer/target optimization combinations;
its commands and results are recorded in `stage1-vector-results.json`.

The type inventory includes ordinary library API gaps, projections, trait
lifetimes and coherence. Later implementations have focused regression coverage;
rerun this maintained audit on a stable snapshot to establish a new complete-graph
count. These inventories are not a native self-compilation or bootstrap claim.
