# Full backend readiness

Run `cargo test -p flux-cli --test keel_backend_readiness -- --ignored --nocapture`
on arm64 macOS. The bounded audit leaves reports in
`out/keel-backend-readiness/<parent-pid>/`. It is ignored by default while the
reported input and library blockers remain.

The frozen `graph.json` records the 39-package normal dependency closure of
Keel assembler, IR, codegen, frontend, optimizer and linker, at the workspace's
pinned revision. Package IDs, versions, source identities, editions, features,
and aliased dependency edges are checked against locked offline Cargo metadata
filtered for `aarch64-apple-darwin`. Target filtering excludes inactive edges
such as serde_core's `cfg(any())` dependency. Duplicate hashbrown versions keep
separate source identities and dependency indices.

Cargo builds the existing workspace as the Rust reference. Its JSON messages
provide actual library artifacts, generated output directories and build-script
cfgs. Reference artifacts are passed only to rustc. Flux receives original source
roots and generated Rust text, never the reference's compiled libraries.
`patterns.txt`, package manifests, the workspace lockfile, embedded libraries,
source registry, consumer, graph configuration, original Rust sources and
reported generated Rust files are guarded byte-for-byte. Cached Cargo build
outputs are recorded as supplied inputs; the test does not claim every build
script executed afresh. SDK discovery and reference commands are also retained.

The single consumer exercises real FunctionBuilder construction, IR verification,
full optimization, AArch64 and x86 code generation and assembly, object parsing,
and Mach-O executable linking with the actual SDK's libSystem TBD interface.
The reference runs both the complete consumer and its linker-free configuration.
Generated executable instructions are not executed.

## Input admission and core audit

The complete graph now admits host procedural macros through the matching
toolchain server. Cargo builds serde_derive as host code; its expanded tokens
enter ordinary Flux collection and checking. Host-only syn, quote, proc-macro2
and unicode-ident dependencies are excluded from the target source closure.
The complete metadata inventory still records all 39 packages. The target worker
loads 34 source crates and one host export root. The macro dylib and bridge source
are included in the byte guard; `host-macros.json` records the artifact.

`SourceDb::set_crate_env` supplies isolated per-crate environments. The audit
passes real Cargo OUT_DIR and rustc-env outputs to each source crate; it does not
combine generated directories or mutate the worker's process environment.
`input.diagnostics` is now empty. Full graph syntax/HIR can run; types, MIR and
codegen remain gated when ordinary source diagnostics exist. This is a supplied
source-graph audit, not a package manager or rustc rlib bridge.

A separate 18-package core closure excludes the linker and its additional
Serde/crypto dependencies. It retains the complete assembler, codegen, frontend,
optimizer, IR and their actual dependencies. An isolated worker loads original source roots with per-crate environments and
real Cargo cfgs.
`core-configuration.json` identifies this subset. Core codegen and native
comparison are gated on clean syntax, HIR, types and MIR. A successful core audit
would still not establish a full-backend native milestone.

## Observed first audit

`out/keel-backend-readiness/10644` completed in 1.99 seconds. The full and core
rustc consumers compile and run successfully. The full consumer reports:

```text
aarch64 backend text=28
x86 backend text=17
linker executable architecture=Aarch64
```

The 18-package Flux core has zero syntax diagnostics and four HIR diagnostics:
one missing std::env::current_dir call in keel-codegen/src/dwarf.rs:66 and three
missing std::cell::OnceCell uses in keel-opt/src/{analysis,instcombine}.rs.
The required OnceCell surface includes new, get_or_init and get_mut. Types,
MIR and Flux native execution did not run.

The inventory contains 1,869 files and 28,028,761 bytes. Both source and parent
integrity reports are `[]`, as are the pre-reference and pre-Flux reports.
This is a blocker inventory, not a passing full-backend test.

## Standard-library prerequisite update

The new OnceCell and current_dir implementations pass focused ownership,
reentrancy/unwind, and native off/on comparisons. The first refreshed core audit,
`out/keel-backend-readiness/7147`, now using isolated crate environments, reaches zero syntax/HIR diagnostics and 92 type
diagnostics. These include constant trait objects, collection APIs, and inference
cases. MIR and Flux native execution remain gated. Both source byte guards pass.

`flux cargo prepare` now creates a Cargo-backed host-input plan, including actual
build-script outputs and proc-macro dylibs. `flux cargo build` compiles supported
binary source graphs with Flux. Macro execution uses the reviewed matching toolchain bridge. Both Serialize and
Deserialize expand the actual serde_derive dylib in focused tests; the backend
audit also loads its real host exports.
See [the tool's contract](../../../README.md#cargo-backed-source-builds).


## Proc-macro admission update

`out/keel-backend-readiness/70001` has zero input-interface diagnostics, zero
syntax diagnostics and 697 full-graph HIR diagnostics. No proc-macro expansion
failure was reported. The first ordinary blockers are missing BinaryHeap,
LinkedList, CString, net, SystemTime and primitive modules in Serde's std surface,
followed by additional unsupported libc declarations. Full-graph types/MIR and
native execution did not run. The 18-package core still has zero syntax/HIR
and 92 type diagnostics. All source-integrity guards pass. This is the expected
failing readiness test, not a passing full-backend build.
