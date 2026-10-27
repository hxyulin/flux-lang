# Observed readiness

The focused test on 2026-10-25 stopped at HIR in 7.97 seconds.
Syntax had 0 diagnostics; HIR had 138. Type checking, MIR and codegen did
not run. The separately compiled rustc reference passed:

```text
aarch64 macho text=12 relocations=1
x86_64 elf text=11 relocations=1
```

The installed compiler was rustc 1.99.1 (58cf8f9c3 2026-10-13), with host
`aarch64-apple-darwin`. The original scripts emitted object `core_error` and
crc32fast `stable_arm_crc32_intrinsics` and `stable_vpclmulqdq`.
All four input integrity checks were clean, including the parent check.

The final run artifacts are under `out/keel-asm-readiness/62964/`.
`configuration.json` records the exact 13-package input graph with package
IDs, dependency indices, editions, features, source paths and rustc artifact
identities. The consumer is index 13, edition 2024, with direct dependencies
12 (keel-asm) and 11 (object), as listed in README.md. `metadata.json` retains
the locked offline metadata. `source-inventory.json` retains 521 original
files and 9,899,137 original bytes. `consumer.rs` retains the consumer input.
Rustc outputs and the runnable reference are in `oracle/`.

The first HIR diagnostic is cfg-if 1.0.5 `src/lib.rs:66`, byte range
`2089..2090`: `expected identifier, found $` (the diagnostic quotes `$` with
backticks). The next is crc32fast 1.5.3 `src/lib.rs:112`, byte range
`3414..3419`: `cannot find State in specialized`.
Full paths, byte ranges and messages for all 138 diagnostics are in
`hir.diagnostics`. The counts below describe diagnostics, including cascades;
they do not claim 138 independent defects or assign compiler/library work.

| Diagnostic | Count |
| --- | ---: |
| expected identifier, found `$` | 1 |
| cannot find `State` in `specialized` | 1 |
| the `test` attribute is not supported yet | 4 |
| impl Trait in edition 2021 are not supported yet | 4 |
| `self` parameters with a type are not supported yet | 6 |
| cannot find value `self` in this scope | 6 |
| lifetime bounds are not supported yet | 2 |
| transparent structs with multiple or no fields are not supported yet | 6 |
| unresolved import `std::io::Read` | 1 |
| unresolved import `std::io::Seek` | 1 |
| unresolved import `std::io::SeekFrom` | 1 |
| `as _` imports are not supported yet | 1 |
| generic associated types are not supported yet | 70 |
| impl Trait in edition 2018 are not supported yet | 5 |
| `where` clauses on associated types are not supported yet | 10 |
| unresolved import `std::collections::BTreeSet` | 3 |
| cannot find `BTreeSet` in `collections` | 1 |
| unresolved import `std::arch` | 1 |
| cannot find function `vld1q_u8` in this scope | 1 |
| cannot find function `vceqq_u8` in this scope | 2 |
| cannot find function `vdupq_n_u8` in this scope | 2 |
| cannot find function `vorrq_u8` in this scope | 1 |
| cannot find function `vget_lane_u64` in this scope | 1 |
| cannot find function `vreinterpret_u64_u8` in this scope | 1 |
| cannot find function `vshrn_n_u16` in this scope | 1 |
| cannot find function `vreinterpretq_u16_u8` in this scope | 1 |
| cannot find `time` in `std` | 4 |

The missing APIs include `std::io::{Read, Seek, SeekFrom}` in object,
`std::collections::BTreeSet`, `std::arch::aarch64` SIMD intrinsics, and
`std::time` in keel-asm. The build scripts and enabled source features remain
unchanged. The test fails at the first dirty stage and does not bypass these
blockers to reach codegen.

Validation command: `cargo test -p flux-cli --test keel_asm_readiness -- --ignored --nocapture`
(run through the installed local mbx wrapper). No formatting, full workspace
validation, compiler/library edits, commits or public actions were performed
for this harness.

This remains an opt-in progress audit while the required language and library
features are outside the supported subset. It is ignored by the ordinary regression
suite; explicit execution still fails at the first dirty compiler stage. The
refactor validation rerun reported 152 HIR diagnostics with unchanged source bytes,
including GATs, CRC feature detection/intrinsics and missing std APIs. No native
assembler milestone is claimed. The implemented asm-ir and Keel IR graphs have
separate active regression tests.

## Resumed audit on 2026-10-27

The guarded audit in `out/keel-asm-readiness/49032` ran after adding typed
receivers, anonymous imports, Read/Seek traits and monotonic time APIs.
Syntax has 0 diagnostics and HIR has 132, down from the preceding 152-diagnostic
inventory. Both worker and parent source-integrity reports are `[]`. The rustc
consumer passed. Types, MIR and native Flux codegen did not run.

The remaining diagnostics are 80 GAT/associated-where gates, 20 CRC feature or
intrinsic errors, 11 architecture/SIMD names, 9 pre-2024 return-position
`impl Trait` gates, 6 multi-field transparent wrappers, 4 test attributes and
2 lifetime bounds. These are diagnostic counts, including possible cascades.
There are no remaining typed-receiver, anonymous-import, Read/Seek or time
name-resolution errors in this inventory.

The ignored parent test now passes `--ignored` to its worker and requires an
inventory artifact. Without those checks an explicitly invoked parent could
return success after its worker skipped the audit.

## Transparent wrappers, test attributes and architecture reexports

The next guarded audit, `out/keel-asm-readiness/20836`, has zero syntax
diagnostics and 111 HIR diagnostics. It ran in 4.70 seconds. Both source-integrity
reports are `[]`, and the separately compiled rustc consumer passed. Type
checking, MIR and Flux native execution remain gated by HIR.

The 21 removed diagnostics were six multi-field transparent wrapper gates,
four ordinary-build test attributes and eleven scanner architecture imports.
Transparent wrappers now admit additional proven size-zero, alignment-one
fields and preserve the represented field's layout and niche. Test functions
are removed before collection in ordinary builds. `std::arch` names the
existing core architecture definitions.

The remaining 111 diagnostics comprise 80 GAT/associated-where gates, 20 CRC
feature or intrinsic errors, nine pre-2024 return-position `impl Trait` gates
and two lifetime bounds. These are diagnostic counts, not independent defects.
The complete assembler still has no native milestone.

## Trait bounds, older-edition opaque captures and CRC

The guarded audit in `out/keel-asm-readiness/7509` has zero syntax diagnostics
and 80 HIR diagnostics, down from 111. It ran in 5.21 seconds. Both worker and
parent source-integrity reports are `[]`, and the rustc consumer passed.
Type checking, MIR and Flux native execution still do not run on this graph.

The removed diagnostics were two trait lifetime bounds, nine older-edition
return-position `impl Trait` gates and 20 CRC feature/intrinsic errors. All 80
remaining HIR diagnostics concern GATs: 70 generic associated type declarations
and ten associated `where` clauses.

The separate active `crc32fast_readiness` test compiles the unchanged crc32fast
1.5.3 and cfg-if 1.0.5 roots with their actual editions, `std` feature and
build-script cfg. It checks baseline/specialized paths, stream crossovers and
combination, then compares native results with Rustc in both optimization modes.
CRC runtime detection uses the actual CPU query. The CRC intrinsics currently
use exact scalar folding, without hardware acceleration. The required shared
slice `align_to` API preserves alignment, element boundaries and input lifetime.

## Associated type families

The guarded audit in `out/keel-asm-readiness/91406` has zero syntax and HIR
diagnostics and reaches type checking. It ran in 16.52 seconds. Both source
integrity reports are `[]`, and the independently compiled rustc consumer passed.
The 80 GAT/associated-where HIR diagnostics from the previous inventory are gone.

Type checking reports 111 diagnostics, including cascades:

| Diagnostic | Count |
| --- | ---: |
| lifetimes in supertraits and associated type bounds | 99 |
| lifetimes in generic arguments of bounds | 3 |
| missing `alloc::collections::TryReserveError` | 3 |
| region-bearing inherited types outside function signatures | 2 |
| lifetime parameters on traits outside the admitted profiles | 2 |
| missing `std::io::Write` | 2 |

MIR and native Flux codegen did not run. The graph remains an ignored progress
audit. Separate active GAT tests cover lifetime/type/const families, selected impl
header substitution, required and output bounds, lending, iterator item equalities
and owner-lifetime associated bindings. The native lending consumer matches rustc
with optimization off/on; this does not prove the whole object dependency graph.

## Tracked trait lifetime bounds

The guarded audit in `out/keel-asm-readiness/24620` reaches type checking with
zero syntax/HIR diagnostics and seven type diagnostics, down from 111. It ran in
19.39 seconds. Both source-integrity guards are `[]`, and the rustc consumer passed.
The remaining diagnostics are three `TryReserveError` uses, two missing
`std::io::Write` uses and two region-bearing inherited associated types outside
function signatures. MIR and Flux codegen did not run.

Ordinary bounds such as `R: ReadRef<'data>` on nominal types now retain their
predicate rows. Value and declaration checks prove them under the original
owner environment, including associated constants. The ReadRef and ObjectSymbol
method shapes admit safe default bodies and represented method-local generic
bounds. Static well-formedness requirements from impl self/header arguments and
function parameter/result types are available inside those declarations; callers
must still satisfy them. This also removes the 88 static-WF diagnostics exposed
by the intermediate audit `out/keel-asm-readiness/98494`.

True lifetime-bearing supertrait edges, function-pointer-contained nominal
contracts and unrepresented projection equalities remain explicitly unsupported.
Separate native consumers cover the implemented pieces, not the whole assembler.

## Body type-checking inventory

The guarded audit in `out/keel-asm-readiness/62811` resolves all seven previous
signature diagnostics. Syntax and HIR diagnostics remain zero. Rustc compiled
and executed the reference consumer; both source-integrity guards are `[]`.

Body type checking now runs and reports 257 diagnostics, including cascades.
The byte-string pattern family accounts for 42 explicit gates and 31 same-site
array/slice mismatches. Other direct prerequisites include 60 slice/Vec/iterator
API diagnostics, 30 BTreeMap API diagnostics, 21 Cow/string comparison errors,
and three boxed-error conversions. Missing APIs also cause inference and
unsized-slice cascades. These counts group messages rather than proving each
root cause. `type-summary.json` in the audit directory groups the exact messages.
MIR and native Flux codegen did not run.

The focused changes add fallible Vec growth with unchanged contents on failure,
`TryReserveError`, byte-stream `Write`, and inherited associated item lifetimes
inside callable method bounds. The callable proof checks the selected owner row
and exports captured-row requirements through the existing closure contract.
Wrong-owner, static-input, nested-row and mutable-invariance cases reject.
Separate native tests compare each implementation with rustc off/on.

## Complete body type checking

The guarded audit in `out/keel-asm-readiness/77654` has zero syntax, HIR and
type diagnostics. It resolves all 257 diagnostics from the previous inventory.
Both original-source integrity guards are `[]`; rustc's reference consumer passes.
The upstream source graph is unchanged.

MIR checking completes with 231 diagnostics, grouped in `mir-summary.json`:

| Diagnostic family | Count |
| --- | ---: |
| caller-bound lifetime proofs | 206 |
| lifetimes | 10 |
| generic destructors in const functions | 8 |
| mutable binding requirements | 3 |
| unrepresented associated bound requirements | 2 |
| overlapping mutable borrows | 2 |

The earlier attempt `out/keel-asm-readiness/64857` reached the same clean type
stage but panicked during selected-family proof initialization. The fix closes
only the original signature/header assumptions before projection instantiation;
it does not turn requested family requirements into assumptions. The final
inventory above completes that stage without the panic. Codegen has not run.

Focused comparisons cover byte-string pattern exhaustiveness, static associated
inference, mutable iterator views, collection ownership/panic behavior, cached
sorting, and conversion lifetimes. BTreeMap remains an AVL implementation with
the represented standard API, not a copy of Rust's private B-tree layout.

A separate family/static-callback reduction was observed during investigation:
`out/family-static-callback-review` records Rust rejecting an unsatisfied
higher-ranked impl requirement that Flux currently admits. Version provenance is
unverified. This is an open correctness boundary, not a verified pre-existing
defect or a fix claimed by this wave.

## Complete MIR and native execution

The guarded audit `out/keel-asm-readiness/4907` passes in 46.70 seconds.
Syntax, HIR, type and full MIR diagnostics are all zero. The unchanged source
inventory contains 524 files and 9,964,611 bytes. Both original-source integrity
guards are `[]`, and `phase.txt` is `native-verified`. Native Flux execution with
optimization off/on matches rustc's status, stdout and stderr. The maintained
readiness test is now active by default.

The consumer assembles AArch64 into Mach-O through both assembler APIs and x86
Intel syntax into ELF. It reads architecture, object format, relocatable kind,
text bytes, relocations and defined/undefined symbols through the real object
crate. Generated instruction streams are not executed.

The 231 preceding MIR diagnostics reduce to shared compiler and library causes:

- Caller bounds on compound self types and inside closures use their original
  lifetime rows, rather than selecting unrelated blanket impl prerequisites.
- Abstract GATs in signatures carry declared well-formedness requirements;
  selected concrete projections and all callers still prove applicability.
  Generic associated outputs include their implicit Sized contract.
- Nested region-free associated selections prove their own impl predicates.
- Proven Copy aggregates do not acquire destructor requirements from unresolved
  associated fields. Genuine owned destructors remain subject to const checks.
- Slice search/sorted callbacks use the receiver lifetime, matching Rust.
- Legacy unique captures retain exclusivity without requiring the reference
  binding to be mutable. Capture metadata uses stable destination locals instead
  of basic-block positions invalidated by drop elaboration.
- Drop-only discriminant reads keep destructor requirements but do not extend
  inactive payload loans. Ordinary matching retains its previous loan liveness.

Focused rustc acceptance/rejection and native comparisons cover these changes.
Independent review caught an overly broad discriminant change; the retained
implementation distinguishes drop-only reads and rejects the reported payload
loan case across all four editions. Earlier FnPtr and family/static-callback
correctness boundaries remain outside this milestone. No package manager,
backend revision change or upstream source rewrite is involved.

A separate review probe found a missing conflict check when an ordinary match
reads enum storage while an exclusive reference to that storage is still live.
The saved case and compiler outputs are in `out/mutable-review`. Its version
provenance is unverified; it is not fixed or claimed as supported by this wave.
