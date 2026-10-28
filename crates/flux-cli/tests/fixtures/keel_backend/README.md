# Full backend readiness

Run `cargo test -p flux-cli --test keel_backend_readiness -- --ignored --nocapture`
on arm64 macOS. The bounded audit leaves reports in
`out/keel-backend-readiness/<parent-pid>/`. It is ignored by default while the
reported input and library blockers remain.

Set `FLUX_BACKEND_ONLY=core` or `FLUX_BACKEND_ONLY=full` to audit just one
source closure while investigating a blocker. The report records that selection;
a core-only pass does not establish full-backend support.

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

## Parallel prerequisite work

The integrated audit `out/keel-backend-readiness/42376` records zero input and
syntax diagnostics. The full graph now has zero HIR diagnostics, down from 697.
The unchanged 18-package core has zero HIR, type and MIR diagnostics; its isolated
worker passes native comparison with optimization off/on in 147.65 seconds.
Both runs print `aarch64 backend text=28` and `x86 backend text=17`, matching the
separate rustc reference. All five byte guards pass.

Focused tests cover constant trait objects, generic callable-object coercions,
enum constant patterns, type/const GAT equality bindings, and non-callable
higher-ranked trait bounds. The library additions include collections, C strings,
address values, system time, output cursors, files, byte/halfword atomics,
volatile scalar accesses, and SHA/NEON helper semantics. Native comparisons use
both optimization tiers; this evidence does not replace the guarded graph audit.

The integrated source includes packed layouts, foreign static imports and native
library linking, function-address integer casts, and byte compare-exchange.
Named-constant aggregate promotion retains allocation alignment and field offsets;
closure result analysis preserves correlated outputs through nested callbacks.

The full worker stops at type checking with 2,485 diagnostics. These are not
2,485 independent blockers: large groups cascade from associated array lengths,
missing `wrapping_offset` pointer methods, additional const pointer/slice APIs,
union field restrictions and remaining trait/projection contracts in Serde and
crypto dependencies. The combined readiness test still fails for that full scope;
full MIR/codegen and linker-native comparison have not run. The core pass is a
backend dependency milestone, not a complete compiler bootstrap.

Current bounded APIs reject aggregate volatile accesses and lifetime-bearing
GAT equality arguments. SHA helpers use scalar implementations, not accelerated
instructions. `hint::spin_loop` currently uses a no-op hint fallback; atomics
still supply synchronization.


## API and array-length follow-up

The full-only audit `out/keel-backend-readiness/16654` records zero input,
syntax and HIR diagnostics and 209 type diagnostics. All four recorded byte
guards are clean. There are no remaining diagnostics for associated array
lengths, array Borrow/Index/IndexMut, borrowed slice-to-array conversions,
wrapping_offset, the added ordered collection/path APIs
or the supplied numeric/error APIs. These groups were cleared through genuine
library implementations and the existing checked anonymous-constant evaluator.

Full MIR/codegen remain gated. The remaining inventory includes Serde lifetime
and projection contracts, typenum normalization, unsupported union fields, PhantomPinned and constant pointer add.
The pointer slice supports bounded constant raw slice construction; it does
not claim general constant pointer arithmetic. Saved per-message/per-crate
counts are in this audit's blocker-summary.json.


## Union and normalization follow-up

The full-only audit `out/keel-backend-readiness/78346` records zero input,
syntax and HIR diagnostics and 59 type diagnostics. All four byte guards pass.
Typenum has no remaining diagnostics, and the union-field restriction group is
cleared by concrete tuple, float, char and thin raw-pointer fields. Generic,
reference-bearing and wide-pointer unions remain explicitly unsupported.

The separate maintained `typenum_readiness` harness proves unchanged typenum
1.20.1 with the actual `const-generics` feature through native execution against
rustc, off/on. Full backend MIR/codegen still wait on Serde lifetime/alias
contracts, crypto associated bounds, itoa inference and missing library APIs.
This audit does not establish a complete compiler bootstrap.

## Owned library APIs follow-up

`out/keel-backend-readiness/66015` records zero input, syntax and HIR diagnostics
and 55 type diagnostics, with all four byte guards clean. Queue reservation,
boxed path/OS-string conversions and `fmt::Error: Error` clear four Serde
API diagnostics. Native `backend_owned_apis` covers both optimization modes.
The full backend remains gated before MIR/codegen by the remaining diagnostics.
