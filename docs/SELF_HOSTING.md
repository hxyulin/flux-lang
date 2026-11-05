# Compiling Flux with Flux

Flux built optimized stage 1 and stage 2 from the complete compiler source
graph with unmodified Keel `a078986ccddb1394db73b42843b282f271807ae4` on
arm64 macOS. The proof uses commit `1aacb10` plus the library support patch
recorded in `out/bootstrap-keel-a078986c-retry/bootstrap-support.patch`.
The new Keel requires `VecDeque::drain` and slice `split_last_chunk`; the first
attempt from the commit alone stopped on those missing APIs. The retry copies
only those library additions and their regression tests into a private source
snapshot. It excludes the separate compiler profiling changes.

Both generations rebuilt 72 source crates with five host procedural macros,
using `flux build --offline -O`. Stage 1 took 200.81 seconds; stage 2 took
about 1,220.18 seconds. Stage 2 completed without a generation timeout.
The test-runner interruption left the compiler running; a replacement runner
captured its successful exit through a Darwin process-exit event. The stage-2
time is recovered from the wall clock and includes subsecond runner bookkeeping.

Both compilers passed help/version checks. Stage 1 passed a smoke comparison
in both output optimization modes. Stage 2 passed seven programs in both modes
against fresh rustc references: smoke, BTreeMap, pointer/slice helpers, trait
lifetime keys, slice or-pattern bindings, layer associated-type bounds and the
new collection APIs. All 14 stage-2 comparisons matched stdout, stderr and exit
status. The stage-1 and stage-2 executables are byte-for-byte identical, with
SHA-256 `79e16d3732434d79eafb72e137d9a960cd3fa60752baa7571a2d9ff3c84458ab`.

Results, commands and artifact hashes are in
`out/bootstrap-keel-a078986c-retry/status.json`. All 6,868 inventoried source and
host-artifact files remained unchanged. Four generated build-script files were
inventoried during stage 2 and remained unchanged through verification; they
were not guarded before generation. The supporting library changes passed
formatting, clippy, runner units, all 474 test targets and workspace/example
source checks, recorded in `out/bootstrap-library-validation/results.json`.
This proof covers the recorded support patch on arm64 macOS, not Linux
self-hosting or later source edits. Cargo and rustc still supply host build
scripts and procedural macros.

## Linux stage 2

On 2026-10-31, Flux built optimized stage 1 and stage 2 on native x86-64 Linux
with unmodified Keel `a078986c`. The private snapshot uses commit `1aacb10` plus
`out/bootstrap-linux-a078986c/support-changes.patch`; its exact hash and file
inventory are recorded in `snapshot.json` beside that patch. The patch includes
the earlier collection API additions, enables Linux Cargo source builds, adds
slice `as_rchunks`, and checks compound legacy intrinsic const arguments through
the existing constant evaluator. Dependency sources and Keel were not patched.
The separate compiler profiling changes are excluded.

Both stages rebuilt 71 source crates with five host procedural macros using
`flux build --offline -O`. Stage 1 took 332.93 seconds and stage 2 took
1,998.76 seconds, without a generation timeout. Both passed help/version checks;
stage 1 also passed a smoke comparison in both output optimization modes.

Stage 2 passed ten programs in both output optimization modes against fresh
rustc references: smoke, pointer/array APIs, integer SSE/AVX, x86 crypto,
BTreeMap, pointer/slice helpers, trait lifetime keys, slice or-pattern bindings,
layer associated-type bounds and the new collection APIs. All 20 comparisons
matched stdout, stderr and exit status. A direct byte comparison confirms that
the stage-1 and stage-2 executables are identical.

The source plans match across stages. All 6,423 inventoried source files,
host-macro artifacts and generated build-script files stayed unchanged. The
independent inventory was captured during stage 1, before stage 2 began, and
checked again after verification. Full validation of the supporting changes
passed all 474 targets on both native hosts, plus formatting, clippy, runner
units, workspace/example source checks and docs.

The portable result is `out/bootstrap-linux-a078986c/proof.json`. Commands,
plans, source guards and comparison logs are copied under its `evidence/`
directory. These results cover the recorded source snapshot and optimized
compiler builds. Cargo and rustc still supply host build scripts and procedural
macros.

## Earlier proofs

The following results describe earlier compiler snapshots and Keel pins.


An earlier guarded proof is `out/flux-compiler-readiness/81431`. Syntax, HIR,
type, MIR and code-generation diagnostic reports are empty, and both final
source-integrity reports are clean. That regression run covered 469 targets. The later process/channel runtime
changes passed all 470 targets on both arm64 macOS and x86-64 Linux. The historical locally patched proof below records the older
`f37f0bd` snapshot.

Earlier stage-2 proofs used Keel `103c23de` on arm64 macOS.
The unoptimized build in `out/bootstrap-stage2-unlimited` passed help/version
and three native compile/run comparisons. The optimized build in
`out/bootstrap-stage2-optimized` rebuilt all 72 source crates with five host
proc macros in 1,038.96 seconds. It passed help/version, six native comparisons
covering both output optimization modes, and 45 timed compile/run comparisons.
All 4,258 inventoried inputs remained unchanged. Those sources were restored
into a private workspace for the optimized run; this does not certify later
source changes or Linux self-hosting.

Optimized stage 2 compiles the three benchmark programs in essentially the
same time as the stock-Keel stage 1. The experimental inlining-budget change
used to generate the faster stage-1 artifact is not part of the pinned target
source graph. These timings compare Flux executables built by different
compilers, not Flux's compilation speed against rustc itself.

The Linux source-build attempt prepared 71 source crates and five host proc
macros, with no dependency-plan blockers. A diagnostic bypass of the Cargo
host gate exposed missing x86 intrinsics, feature attributes and detection,
inline assembly, and Linux runtime APIs. No Linux self-built compiler was
produced. After the process and channel port, a source-guarded retry still
prepared the same 71 crates and five host proc macros. Process/channel import
and host Send/Sync errors are gone. The integer SSE/AVX2 bridge now passes native
comparisons, and unchanged hashbrown sources and the feature-free memchr wrapper
execute on Linux in both optimization modes. Function-local ISA permissions and
runtime feature detection are supported. A later source-guarded retry still
prepared 71 source crates and five host proc macros. It reached diagnostics for
compound legacy constant arguments, SHA and carry-less multiplication APIs,
AVX-512, x86 inline assembly and `remove_dir_all`. Argument capture remains a
runtime gap in that audit. Linux now captures argc/argv at the generated entry
point, including when main calls into a separately compiled path dependency.
Recursive removal and bounded integer inline assembly pass native comparisons;
the new removal implementation also checks symlink replacement races and
rejects a trailing slash on a root symlink before touching its target.
Cargo and rustc still supply host build scripts and procedural macros.

The retry after those Linux changes prepared 71 source crates and five host proc
macros, with no plan blockers and unchanged source guards. The x86 assembly and
recursive-removal diagnostics are gone. The remaining diagnostics concern crypto
APIs/features, AVX-512 and compound legacy constants. No Linux self-built compiler
was produced. Full validation passed all 473 targets on both native hosts, plus
formatting, clippy, runner units, workspace/example checks and codegen docs.

## Executable source graphs

The maintained native tests run on arm64 macOS. Each loads production Rust
sources into Flux, builds native executables with optimization disabled and
enabled, and compares output and exit status with separately built rustc
executables. Rustc supplies the oracle; Flux does not use rustc to compile
these source graphs.

| Test in `crates/flux-cli/tests` | Source coverage |
| --- | --- |
| `run`, through `just selfhost-subset` | Choice tapes, MIR bitsets and intervals, syntax kinds, parser events, generator types and tree navigation. |
| `lexer_selfhost`, `parser_selfhost`, `tree_pipeline_selfhost` | Unchanged lexer, parser and tree modules through explicit wrappers, plus real memchr and unicode-ident sources. |
| `typed_ast_readiness` | Complete unchanged syntax source inventory, generated typed AST and handwritten accessors. |
| `arena_selfhost` | Complete unchanged arena crate, including owned, shared, layered and dropless arenas. |
| `hashbrown_readiness` | Complete unchanged hashbrown 0.17.1, with default features disabled as requested by Flux. |
| `interner_selfhost` | Complete unchanged interner with arena, hashbrown and rustc-hash sources, including native TLS and eight pthread workers. |
| `typenum_readiness` | Original typenum 1.20.1 in edition 2018 with `const-generics`, arithmetic projections and runtime operators. |
| `smallvec_readiness` | Unchanged smallvec 1.16.3 in its original edition 2018, using the workspace's resolved empty feature set. |
| `hir_selfhost` | Complete unchanged HIR with arena, interner, syntax and dependency sources, original editions and resolved library features. |
| `ty_selfhost` | Complete unchanged type checker and its eleven-library source graph, with public inference and signature consumers. |
| `mir_selfhost` | Complete unchanged MIR crate and its thirteen-library source graph, with public MIR building, borrow checking and constant-evaluation consumers. |
| `keel_asm_ir_readiness` | Complete pinned Keel assembly-IR source with original smallvec sources, structured assembly consumers and byte guards. |
| `keel_asm_readiness` | Complete unchanged pinned assembler and its thirteen-package source graph, including object writing/reading and both hashbrown versions. |
| `keel_backend_readiness`, with `FLUX_BACKEND_ONLY=full` | Complete pinned backend/linker source graph; emits AArch64 and x86 objects and links an AArch64 executable. Explicit ignored audit, with native off/on comparisons and byte guards. |
| `keel_reduced_suite` | Flux-compiled pinned backend runs 19 selected upstream tests and emits ten executable semantic cases per compiler mode. Explicit ignored audit with byte guards. |
| `flux_compiler_readiness` | Complete CLI library, binary and target source graph. The separate current-pin bootstrap audits above verify optimized stage 1 and stage 2 on both hosts with the recorded support patches. |
| `keel_ir_readiness` | Complete pinned Keel IR source, layouts, empty-module parsing and verification. |
| `keel_float_math` | Unchanged pinned Keel floating-point constant folder with the complete Keel IR dependency. |
| `bumpalo_readiness` | Original bumpalo 3.20.3 with the resolved `collections` and `default` features, including collection ownership and zero-sized drops. |
| `equivalent_readiness` | Original equivalent 1.0.3 in edition 2015, with native borrowed-key comparisons. |
| `crc32fast_readiness` | Original crc32fast 1.5.3 and cfg-if 1.0.5, actual `std` feature/build-script cfg, baseline/specialized CRC paths and combining. |

Run a graph directly, for example:

```sh
cargo test -p flux-cli --test typed_ast_readiness
cargo test -p flux-cli --test hashbrown_readiness
cargo test -p flux-cli --test interner_selfhost
cargo test -p flux-cli --test smallvec_readiness
cargo test -p flux-cli --test typenum_readiness
cargo test -p flux-cli --test hir_selfhost
cargo test -p flux-cli --test ty_selfhost
cargo test -p flux-cli --test mir_selfhost
cargo test -p flux-cli --test keel_asm_ir_readiness
cargo test -p flux-cli --test keel_asm_readiness
cargo test -p flux-cli --test keel_ir_readiness
FLUX_BACKEND_ONLY=full cargo test -p flux-cli --test keel_backend_readiness -- --ignored --nocapture
cargo test -p flux-cli --test keel_reduced_suite -- --ignored --nocapture
cargo test -p flux-cli --test keel_float_math
cargo test -p flux-cli --test bumpalo_readiness
cargo test -p flux-cli --test equivalent_readiness
cargo test -p flux-cli --test crc32fast_readiness
```

The typed-AST consumer exercises 135 generated node interfaces, 536 accessors
and 82 conversions. It reads the current parser fixtures and compares complete
observable results, including casts, conversion identity and handwritten
accessors. A recorded run produced identical 830,745-byte outputs.

The hashbrown consumer covers collision-heavy growth, lookup, removal, reserve,
clone, retain, mutable iteration, draining and reuse. It checks reserve overflow,
zero-sized entries and exactly 258 destructor calls, including partially drained
owners. The test checks original and copied upstream files byte for byte.

The interner consumer interns 5,000 names, checks reverse lookup and predefined
symbols, and exercises 512 shared names from eight native workers. Production
interner and arena sources, TLS behavior and native stack sizes remain intact.
The code generator removes redundant aggregate return buffers and copies rather
than changing these source requirements. Separate regressions check Copy
arguments, overlapping return destinations and Drop counts.

Smallvec coverage includes inline and spilled storage, shrinking, insertion,
removal, retaining, cloning, draining, `from_vec`, exported macros, borrowed fat
pointers, exact Drop counts and zero-sized elements. Locked offline metadata
confirms no optional features are selected in the current Flux/Keel graph.
Optional `union` and `const_generics` rustc audits do not establish Flux coverage
for those unselected branches.

The crc32fast consumer covers 90 length/offset combinations, incremental
updates, three-stream crossover lengths and hash combination. Its original
build script supplies the real intrinsic cfg flags to both compilers. Runtime
CPU detection is genuine; CRC folding currently uses scalar code. This proves
source compatibility, without hardware acceleration. The driver calls the
public `hash` function through its qualified path: a separate import-resolution
bug still confuses a private module and a public function sharing that name.

The HIR consumer executes item collection and body lowering, resolves modules
and traits, and compares an unresolved-name diagnostic. It also loads physical
files through SourceDb, expands macros and source/data includes, and checks
per-crate cfg with a feature disabled and enabled. The graph enables rustc-hash's
resolved `default`/`std` features and memchr's `alloc`/`default`/`std` features.
All original source editions and bytes are retained. A native run passed with
optimization off and on after zero syntax, HIR, type and MIR diagnostics.

The assembly-IR consumer exercises AArch64 inline and spilled operands, x86
parsing and printing, validation, directive escaping, cloned operand ownership
and program text coalescing. Its guarded run compared both optimization modes
with rustc after zero frontend diagnostics, retaining 111 source and configuration
files byte for byte. Nested constant-table operands use promoted allocations,
including enums with payload-bearing sibling variants.

The earlier memchr comparison covers 32,640 search/count/substring cases. Its
explicit edition-2024 feature-free wrapper remains a narrower boundary than
loading the original manifest. The interner's rustc-hash graph likewise currently
uses an explicit edition-2024 boundary. Do not infer general manifest support
from a source graph.

The `mir_selfhost` audit extends the original HIR/type graph with unchanged
smallvec and flux-mir sources. Its public consumer builds and checks MIR,
observes borrow and initialization diagnostics, and evaluates constants across
fourteen separately rustc-checked cases. It preserves original editions, resolved
features and every source byte, with integrity checks in both worker and parent.
The guarded native comparison passes with optimization disabled and enabled,
matching rustc output, stderr and exit status after zero syntax, HIR, type and
MIR diagnostics. Both source-integrity checks pass. Associated types are
normalized before structural region rows are split, preserving reference slots
inside projected iterator items. Run its audit with:

```sh
cargo test -p flux-cli --test mir_selfhost
```

## Language and runtime prerequisites

The source milestones required compiler changes rather than algorithm rewrites:

- Internal `may_dangle` promises are restricted to unsafe Drop implementations
  in embedded library crates. Destructor liveness still follows actual owned
  fields and PhantomData, preserving borrows inspected by owned elements' Drop.
  Stable user code cannot enable the attribute.
- Trait lifetime admission happens after complete signatures. Required-method
  callbacks and returned stored references retain their region constraints.
  Universal Send/Sync implementation lifetimes support distinct binder slots;
  specialized, repeated, bounded and region-dependent marker implementations
  remain rejected where their applicability is not represented.
- Conditional compilation prunes fields, variants, receivers and parameters
  before numbering, derives and signature construction.
- Explicit receivers support `Self`, shared/mutable references to `Self`, and
  `Box<Self>`. Boxed method lookup selects the pointee's implementation while
  consuming the original owner. Anonymous `use Trait as _` imports make methods
  available without introducing a name, including through public glob imports.
  Dynamic dispatch through a boxed trait-object receiver remains unsupported.
- Closures can belong to functions, constants and statics. Their bodies execute
  with ordinary runtime capture and unsafe checks. Const construction and
  function-pointer coercions remain checked; const closure calls are rejected.
- Function addresses survive constant evaluation, aggregate constants,
  promotions and generic associated constants. Monomorphization collects them
  from emitted statics and reachable instantiated bodies. Unused constants do
  not introduce unrelated function roots.
- Physical source inclusion supports `include!`, `include_str!`,
  `include_bytes!`, eager `concat!`/`env!` filenames, generated impl items and
  function-block items. Supplied build-script output can be loaded through
  `OUT_DIR`; running build scripts is a separate build input task.
- Native C scalar callbacks and function pointers preserve ABI and safety.
  Natural-layout repr(C) records and nested fixed arrays use ByVal/Sret C
  marshalling, including generic definitions and indirect calls. Other aggregate
  layouts reject before IR generation.
  C functions do not implement Rust's Fn traits. Borrowed function pointers
  autodereference when called.
- Reference-returning function pointers retain canonical input/output region
  recipes. Named and elided binders instantiate freshly per call; free and
  static regions stay distinct. Reification and nested value conversions prove
  callable variance, while trait selection preserves binder identity. These
  signature types do not become owned captures for Drop or Send/Sync checks.
  Region-bearing aliases within pointer signatures, nested pointers capturing
  outer regions, and unrepresented function-item owner provenance still report
  explicit unsupported diagnostics.
- Native `thread_local!` keys initialize per thread and destroy their values
  at thread exit. Access to a destroyed key fails, and callback references
  cannot escape. Qualified and imported std macro paths work.
- OnceLock, LazyLock, Mutex and RwLock support static construction. The locks
  use acquire/release spinning, expose sized guard APIs and do not promise
  fairness. Flux panics abort; separate host tests exercise unwind recovery
  and poisoning. Native contention tests call generated code from host threads.
- Bounded NEON APIs provide the eight-byte operations used by hashbrown and
  the existing 128-bit delimiter pipeline used by Keel. `std::arch` reexports
  those core definitions. Focused native tests cover scanner masks and reified
  calls; the complete assembler graph also passes the native comparison below.
  Transmute checks size at generic definition time and scalar/reference
  validity during represented const evaluation. Const-field promotion retains
  root alignment, offsets and nested function addresses.
- Pointer reification retains its complete target type, including safety and
  ABI. Closure outlives checks visit stored captures rather than signature-only
  generic arguments. Explicit opaque outlives bounds guide storage liveness
  while preserving checks on real captures.

The bounded std facade is described in [its README](../library/std/README.md).
Library and language support remain subject to [LANGUAGE.md](LANGUAGE.md).

## Remaining boundaries

The standalone package driver's `--macro-host-compiler flux` mode compiles
the host token API and dependency-free procedural macro implementations with
Flux. Tests exercise all three macro kinds with rustc replaced by a rejecting
executable, and compare the unchanged published `default_new_macros 0.1.0`
derive with rustc. The host adapter uses the existing HIR proc-macro interface.
HIR and type-checker dependency graphs do not gain the host token runtime.
The package driver reads local manifests without Cargo. A local path graph
with a Flux-built macro host can therefore build without Cargo or rustc.
Cargo-built macros and build scripts retain their existing bootstrap path;
this local mode is not a claim about compiling the full CLI source closure.

The complete HIR, type-checker and MIR source graphs have guarded native
comparison harnesses. Keel IR, assembly IR, bumpalo and the actual Keel
floating-point constant folder also have native comparisons. These milestones
do not yet build the complete compiler executable. The assembler and its
object-file dependencies now also pass native comparisons. The remaining backend
source graphs are complete Keel codegen, frontend, linker and optimizer crates,
followed by flux-codegen. The maintained `keel_backend_readiness` audit records
that 39-package closure, actual build outputs, and a separately scoped
18-package linker-free core. Per-crate compilation environments now provide
separate OUT_DIR values. In `out/keel-backend-readiness/42376`, the unchanged
18-package core has zero syntax, HIR, type and MIR diagnostics. Its Flux-built
consumer constructs IR, optimizes, emits AArch64 and x86 code, and parses object
output, matching rustc with optimization off/on. All byte guards pass. This
does not yet build the complete Flux executable. The full graph now has zero
syntax/HIR diagnostics and reaches type checking with 2,485 diagnostics, including
cascades from associated array lengths, missing pointer APIs, union field support
and remaining serialization trait/projection contracts. Full-graph MIR/codegen
remain gated. `flux prepare` records Cargo build-script
outputs and host proc-macro artifacts, and `flux build` compiles supported
binary source graphs. Function-like, attribute and derive proc macros now expand through the matching
rustc toolchain server. The full backend audit includes the actual serde_derive
host dylib and checks its expanded target source. Host-only syn/quote dependencies
stay outside target compilation. The bridge is tied to a reviewed rustc commit;
its [contract and restrictions](../crates/flux-cli/README.md#cargo-backed-source-builds)
remain narrower than general Cargo compatibility. The rustc reference exercises the complete backend
and linker successfully. See the [backend audit report](../crates/flux-cli/tests/fixtures/keel_backend/README.md).
The API/array-length follow-up audit `out/keel-backend-readiness/16654` has zero
input, syntax and HIR diagnostics, and 209 type diagnostics, with clean byte
guards. Associated-constant array lengths now use the ordinary checked constant
evaluator. Pointer/array borrowing and chunk APIs, ordered collection/path APIs,
array indexing and borrowed slice-to-array conversions, NonZero integer families
and smaller allocation/error APIs clear their earlier
diagnostic groups. Remaining errors include Serde lifetime/projection contracts,
typenum normalization, union fields, PhantomPinned and general constant pointer arithmetic. This full-only audit stops
before MIR/codegen; it does not establish a new native backend milestone.
The trait-projection follow-up `out/keel-backend-readiness/56684` has zero input,
syntax and HIR diagnostics, 166 type diagnostics, and clean byte guards.
Impl and associated-value checking now retain written projection-owner lifetime
arguments and prove their selection against the header bounds. Ten broader
Serde trait-declaration gates, two nested `Self` projection-owner occurrences,
other outlives requirements, typenum normalization and union support still block
the full graph. This audit also stops before full-graph MIR/codegen.

The union/normalization follow-up `out/keel-backend-readiness/78346` has zero
input, syntax and HIR diagnostics and 59 type diagnostics. All four byte guards
pass. Typenum and union-field diagnostics are cleared. Remaining groups include
Serde lifetime and alias contracts, crypto associated bounds, itoa inference,
constant pointer arithmetic, PhantomPinned and missing library APIs. Full-graph
MIR/codegen remain gated.

The owned-library follow-up `out/keel-backend-readiness/66015` has zero
input, syntax and HIR diagnostics and 55 type diagnostics. All four byte guards
pass. `VecDeque::reserve`, `PathBuf::into_boxed_path`,
`OsString::into_boxed_os_str` and `fmt::Error`'s error trait implementation clear
four Serde API diagnostics. The native `backend_owned_apis` fixture compares
wrapped queue growth, owned element destruction and boxed path/string views
against rustc in both optimization modes. The remaining compiler and library
groups still gate full-graph MIR/codegen.

The Serde method-lifetime follow-up `out/keel-backend-readiness/63167`
has zero input, syntax and HIR diagnostics and 48 type diagnostics, with all four
byte guards clean. Generic safe method contracts clear seven declaration gates,
including Deserialize, DeserializeSeed, Deserializer, Visitor and the sequence,
map and variant access traits. EnumAccess's lifetime-bearing associated bound,
the two trait-owned generic/predicate declarations, and other Serde lifetime and
alias diagnostics remain gated. `serde_lifetime_methods` compares a borrowed
visitor/reader/decoder pipeline against rustc off/on and checks rejected lifetime
escapes and incompatible method contracts. Full backend MIR/codegen are not yet
reached. The separate itoa fallback experiment is not part of this implementation.

The associated-output follow-up `out/keel-backend-readiness/91160` has zero
input, syntax and HIR diagnostics and 47 type diagnostics; all four byte guards
pass. Reconstructed ordinary output contracts clear EnumAccess's declaration
gate. The `serde_associated_lifetimes` native fixture preserves borrowed string
identity through an enum/variant-style pipeline, with rustc comparison off/on.
`IntoDeserializer` and `IdentifierDeserializer` remain gated: the owner-generic
prototype exposed missing validation of bounds on trait type arguments and was
not retained. Other Serde lifetime and alias errors still block full MIR/codegen.

The trait-argument well-formedness follow-up
`out/keel-backend-readiness/6818` has zero input, syntax and HIR diagnostics and
45 type diagnostics, with all four byte guards clean. Each written trait use
now checks explicit and implicit-Sized requirements on its type arguments,
using caller projections and declaration sizing information. Associated-family
bounds are checked in their own environments. This permits the represented
region-free owner parameters/defaults of IntoDeserializer and
IdentifierDeserializer, clearing the last two Serde trait-declaration lifetime
gates. Other Serde alias/projection/lifetime diagnostics, crypto contracts, itoa
inference and missing APIs gated full backend MIR/codegen at that stage.
`trait_argument_bounds` covers 45 rustc/MIR cases plus native borrowed reader
execution off/on. The separate itoa fallback experiment remains unapplied.

The integer-fallback and library/proof follow-up
`out/keel-backend-readiness/69577` has zero input, syntax and HIR diagnostics
and **6 type diagnostics**, down from 45. All saved input guards are clean.
The backend core consumer still executes and matches rustc with optimization
off and on; the full graph stops before MIR. Integer fallback now visits trait
arguments as well as Self. RefCell, Mutex and RwLock accept unsized payloads and
guards; PhantomPinned and structural Unpin distinguish payload ownership from
movable references and heap handles. Alias-cycle detection separates qualified
associated names from alias edges, and transitive Self bounds imply Sized.
Selected associated-output bounds require an independent proof of their owning
trait. Impl supertrait type proofs preserve caller equalities, and associated
value signatures retain their owner lifetime namespace. Callback output elision
enters its Fn binder before walking its inputs.

The remaining type diagnostics are two non-const pointer `add` calls in
hybrid-array, its named lifetime in a qualified body path, and three nominal
lifetime uses hidden behind concrete IntoIterator projections in Serde fields.
Pointer offsets need allocation-preserving constant evaluation. The lifetime
cases need explicit occurrence rows; dropping their checks would hide missing
borrow constraints. The focused supertrait/associated-value type tests do not
claim support for every corresponding MIR declaration proof. `Pin` APIs are
not supplied by the new marker support.

The const-pointer follow-up `out/keel-backend-readiness/46845`, run with
`FLUX_BACKEND_ONLY=full`, has zero input, syntax and HIR diagnostics and **4 type
diagnostics**, with all four input guards clean. Both hybrid-array pointer-add
errors are cleared. Raw pointer add/sub/offset and NonNull::add are const-capable;
the evaluator keeps allocation roots and byte offsets rather than converting
addresses to integers. Promoted slices retain their original allocation, static
initializers retain relocation addends, and slice metadata can be read without
reading interior-mutable static contents. The focused suite compares 37 cases
against rustc; native off/on coverage checks static and field identity, one-past
pointers, const-fn splitting, mutable static addresses and generic function pointers.
Wrapping offsets, pointer differences and mutable local raw-memory access remain
outside this const subset. At that stage the full backend stopped before MIR at one
hybrid-array qualified body lifetime and three Serde nominal projection rows.
The expanded MIR consumer in `out/mir-selfhost/14545` also passes natively in
both optimization modes, with zero frontend/MIR diagnostics and both byte
guards clean. Its fourteen cases include evaluated pointer offsets through a
slice and rejection of a nonzero field projection from a one-past pointer.

The qualified-lifetime follow-up `out/keel-backend-readiness/68375`, run with
`FLUX_BACKEND_ONLY=full`, has zero input, syntax, HIR and type diagnostics.
All four byte guards pass. Hybrid-array's qualified reference call now records
its written Self regions; Serde's concrete iterator fields substitute selected
impl header and output region tables. Projection inputs retain invariance and
implied outlives requirements, including when the selected result erases them.
That run reached full MIR checking and reported **258 diagnostics**.
These include 120 unresolved associated-bound proofs, 63 unresolved associated
projection proofs, and projection outlives and header-lifetime requirements.
Crypto-common contributes 171 diagnostics, hybrid-array 27, Serde/serde_core 21,
and itoa one; the remaining 38 are in the rest of the graph.
That run did not reach full-backend codegen or native execution.
The focused qualified-lifetime tests compare 24 cases with rustc, and the native
fixture covers owned and shared iterator fields plus qualified reference calls
with optimization off and on.
The unchanged full Flux MIR source graph also passes again in
`out/mir-selfhost/52937`: all four compiler-stage reports are zero, both native
optimization modes match rustc, and both byte guards are clean.

The MIR follow-up `out/keel-backend-readiness/14205` reduces the complete graph
from 258 to **46 MIR diagnostics**, with zero input, syntax, HIR and type errors
and all four byte guards clean. Associated outputs use their declared static
bounds, including static requirements inherited through output trait bounds.
Type/const-only family projections can use outlives facts about their owner and
all family arguments when those inputs have no region slots. Projections with
lifetime-bearing inputs still require their occurrence tables. Nested selection
proofs discharge child projections before matching normalized outer inputs;
this clears the crypto-common arithmetic declaration failures.
That audit had 18 associated-projection lifetime failures,
11 unresolved associated bounds, eight unresolved supertrait proofs, four
unresolved projection proofs, three impl-signature mismatches and two ADT
trait-lifetime failures. It stopped before full-backend codegen.
The focused regressions compare fifteen output-outlives cases and six nested
selection cases with rustc. Native off/on coverage includes borrowed owners,
static outputs and generic array-family borrows. The typenum consumer also checks
associated declarations using `Prod<U3, U2>` and `Prod<U13, U4>`.

The full backend first passed MIR in `out/keel-backend-readiness/76539`, run with
`FLUX_BACKEND_ONLY=full`: input, syntax, HIR, type and full MIR reports all contain
zero diagnostics. All five byte-integrity guards pass, including the check before
codegen. That run stopped in unoptimized codegen with twelve unsupported C ABI
layout diagnostics in libc declarations. These concern nullable function-pointer
callbacks and libc enums. It did not reach full-backend native execution.

The final MIR fixes preserve each abstract associated output's owner and family
input rows while proving its declaration bounds and supertraits. Impl definitions
are substitutions when checking required equalities; required equalities are not
assumptions. Original header bounds supply their type-only supertrait equalities.
Selected RHS substitutions remain available after their lifetime requirements are
proved, including when impl matching infers an iterator item containing a caller's
rigid generic parameter. Declaration ADT checks walk normalized output types with
their output rows. Forty focused rustc cases cover forwarding, signatures, family
prerequisites, inherited equalities, repeated projections and rejection cases.
The final constant-evaluation blocker was Keel's `PASSES` table: trait-object
unsizing now preserves offsets into named constant slices, including function
pointer fields. Nineteen const-object cases and native off/on tests cover indexed
views, raw null/one-past pointers and rejection of invalid references.

The ABI follow-up `out/keel-backend-readiness/58646` passes the complete
backend graph, including object generation and native execution with optimization
disabled and enabled. Input, syntax, HIR, type and MIR reports contain zero
diagnostics; all five byte-integrity guards are clean. Both native runs match
rustc: AArch64 text size 28, x86 text size 17, and an AArch64 executable linked
through the real Keel linker and SDK libSystem stub. This proves the exercised
backend consumer, not a complete Flux compiler bootstrap.

The reduced upstream suite in `out/keel-reduced-suite/49440` passed 19 selected
SSA-builder, rewrite-verifier, DCE and call-ABI test bodies. The test bodies are
copied unchanged from pinned upstream sources, with only test registration
removed. Each of the rustc reference and both Flux-built backend modes also
generated ten AArch64 programs covering loops, calls, memory and arithmetic.
All 30 programs executed successfully. X86 objects were inspected, not executed.
All frontend reports were zero and all five byte guards were clean. The latest
run completed in 203.21 seconds. The
[selection report](../crates/flux-cli/tests/fixtures/keel_reduced_suite/README.md)
records the source adaptations and coverage; this is not the entire Keel suite.

C call boundaries now marshal fieldless enums with explicit C/integer
representations through their physical integer tags, and core nullable function
pointers through one pointer. Rust's internal discriminant/payload leaves stay
private. Native direct/indirect calls, returns, callbacks and C record fields
cover signed/unsigned tags, safe/unsafe nullable pointers and null cases. Eleven
unsupported-shape cases retain fail-closed admission. The harness supplies the
same SDKROOT to both reference and Flux executions; its bounded worker budget
covers frontend checking and both object-generation tiers.

The maintained `typenum_readiness` test compiles unchanged typenum 1.20.1 in its
original edition 2018 with the resolved `const-generics` feature. Its consumer
checks arithmetic projections, lengths and runtime subtraction/division/remainder.
In `out/typenum-readiness/78705`, all compiler stages report zero diagnostics,
both native optimization modes match rustc, and both byte guards pass. Projection
normalization now canonicalizes environment inputs, preserves abstract caller
outputs, and carries trait defaults through aliases. Nested region-free output
bounds are proved before normalization; region-bearing selections retain their
existing occurrence checks.

CLI derives and the backend's remaining dependencies and generated outputs also
belong to the complete source closure. Sourcegen and difftest are additional workspace targets,
not prerequisites for the first compiler executable.

The guarded assembler audit, `out/keel-asm-readiness/4907`, passes with zero
syntax, HIR, type and full MIR diagnostics. Both source-integrity guards are
`[]`. Flux compiles the unchanged pinned assembler dependency graph and runs
the consumer with optimization off/on; status, stdout and stderr match the
separately compiled rustc reference. The consumer writes AArch64 Mach-O and
x86 ELF objects and reads their sections, relocations and symbols through the
actual object dependency. It does not execute the assembled instruction streams.

The preceding 231 MIR diagnostics are resolved through caller/closure contracts,
GAT signature well-formedness and implicit Sized bounds, nested associated-value
proofs, Copy-aware const-drop checks, receiver-correlated slice callbacks, and
closure capture metadata that survives drop elaboration. Drop-generated enum
reads have separate liveness from ordinary matches; payload borrow and destructor
rejection cases remain covered. This is an assembler dependency milestone, not
a complete compiler bootstrap. See the
[readiness report](../crates/flux-cli/tests/fixtures/keel_asm/READINESS.md).

The focused `gat_lending` native comparison exercises shared/mutable lending,
iterator item families, nested symbol-table families, independent data/receiver
lifetimes and destructor counts. It matches rustc with optimization off/on.
Family type/const arguments and contract rejection cases have separate type and
MIR oracle suites. This is bounded GAT support, not an object-file or complete
assembler native milestone.

Tracked trait bounds now preserve owner lifetime arguments and regions inside
type arguments on functions, impls and nominal types. Nominal contracts are
checked in values, annotations, signatures, nested declarations, type arguments
and associated constants. Safe default methods cover borrowed receivers and
Copy by-value receivers with represented method-local generics. Static WF facts
from signature and impl-header types remain distinct from caller obligations.
The `adt_trait_regions`, `safe_trait_defaults`, `trait_type_arguments` and
`static_signature_wf` native consumers match rustc off/on.

Current bounded language implementations still reject unsupported shapes.
The earlier function-pointer findings about object defaults across containers
and implied-bound output conversions have regression tests. Fresh comparisons
of the original examples agree with rustc; this does not establish support for
all function-pointer lifetime forms.
A separate existing formation-check gap accepts a discarded static-bounded
aggregate such as `struct U<T: 'static>(T); let _ = U(&local);`. Rust rejects it.
The explicitly ignored `discarded_static_aggregate_requires_wf` test in
`crates/flux-mir/tests/static_signature_wf.rs` retains this negative reproducer;
the active signature-WF tests still enforce static requirements at calls.
Examples include unproven zero-sized transparent marker fields, broader dynamic
callable lifetime forms, generic associated-constant promotion, raw-slice construction
in constant evaluation and generic unions. The dynamic callback extension
currently covers direct elided references to concrete region-free inputs,
region-free outputs and closure-source coercions. This is enough for the
exercised HashTable graph, not every Rust trait object.

Virtual-source inclusion and include calls first resolved after body-item
collection remain explicitly rejected. Const memory interpretation is typed and
bounded; it is not a general byte-memory evaluator. Scoped and unscoped Rust
thread creation use pthreads. Worker panics abort; unwinding remains outside the
runtime subset.

`fluxc build` compiles one root and its modules with embedded core, alloc and
bounded std. The separate restricted [package driver](PACKAGE_DRIVER.md) accepts
some offline local path graphs. Expanding registry/git resolution, feature
resolution, package artifacts and serialized metadata is deferred. Explicit
crate configuration and source dependency inputs can be extended independently
of that driver. Linking rustc-built Rust libraries is not a substitute for
compiling the required sources.

The backend emits and links native arm64 macOS and x86-64 Linux executables.
Linux uses Keel's ELF linker with installed glibc startup objects and shared
libraries located through `cc`. Linux package builds are supported; the Cargo
source driver remains gated to arm64 macOS. Linux self-hosting still needs
crypto extensions, AVX-512 and compound legacy constants. Argument capture,
recursive removal, bounded integer inline assembly, process spawning, owned pipes and channels
have native comparison coverage on glibc x86-64 Linux. A rustc-built backend
can help diagnose
frontend milestones, but leaves part of the compiler built by rustc. The final
source closure includes Keel and its required build outputs.

## Bootstrap criterion

The complete MIR crate graph in `out/mir-selfhost/41292` passed in 250.14
seconds. All four frontend reports were empty, both native modes matched rustc,
and both byte guards were clean. This covers the MIR consumer and its dependency
graph.

The guarded type-checker graph in `out/ty-selfhost/53293` passed in 77.74
seconds. Syntax, HIR, type and MIR reports were empty; native output and exit
status matched rustc with optimization disabled and enabled. Both byte guards
were clean. This verifies the type-checker consumer, not the complete CLI.

The complete compiler audit in `out/flux-compiler-readiness/61310` reached zero
syntax, HIR, type and MIR diagnostics, then stopped at zmij's SIMD inline assembly.
The pinned Keel verifier rejected vector operands in SIMD registers, and its
AArch64 template printer rejected the `v` register modifier.

The local-patch run in `out/flux-compiler-readiness/665` passed in 379.15 seconds.
The [maintained audit](../crates/flux-cli/tests/fixtures/flux_compiler/README.md)
loaded the CLI library and binary as separate source roots across 70 target
package identities. Syntax, HIR, type, MIR and both lowering reports were empty.
Both resulting Flux executables matched rustc's CLI help and version output,
compiled the smoke program, and executed it with matching stdout, stderr and
status. The source, parent and pre-codegen byte guards were clean. A further
vector-assembly regression passed all four combinations of stage-1 compiler and
generated-program optimization, including mixed scalar/vector outputs.

This run used a temporary Keel checkout based on `f37f0bd`, with two backend
changes: accept vector operands for SIMD assembly constraints and print AArch64
`vN` template operands. Flux packs its memory-backed vector wrappers at the
assembly boundary and stores mixed scalar/vector outputs in one complete result
tuple at the physical field offsets. Cargo's ignored local path patches select that checkout;
the permanent manifest pin has not changed. The audit records the checkout,
backend diff and actual input bytes. This is evidence for the recorded local
backend, not the unmodified pinned backend.

The next bootstrap step is to run regressions through the stage-1 compiler and
use it to rebuild the complete compiler graph. Stage-1 CLI and smoke success do
not establish stage-2 equivalence or eliminate the rustc/Cargo host tooling.


### Current Keel pin: frontend progress

The guarded complete compiler audit in `out/flux-compiler-readiness/43340` uses
unmodified Keel `103c23de480d76e02267c5a8d5a9d3505bc73231` and its bbtree/Verus
dependency closure. It reports zero syntax and HIR diagnostics and 83 type
diagnostics in 178.69 seconds. Both final source-integrity reports are empty.
MIR and native compilation remain gated; this run does not establish stage 1
for the current pin.

The remaining type diagnostics include NonNull byte/provenance/copy helpers,
slice chunks, Result borrowing,
string/path helpers, unsized bounds and region-admission limits.
Trait return-position impl Trait now passes native generic/default/borrowed-output
regressions, contract rejection tests and saved-library reuse. Input-only assembly
passes scalar/vector native regressions. The embedded future/pin/task modules and
pin macros resolve. Verus
macro reexports, generated attributes, preserved input token contexts, anonymous
lifetimes and underscores now expand. Inactive negative impls parse and are removed
by cfg; active negative impls remain explicitly unsupported. The maintained graph
also records flux-codegen's normal dependency on flux-ty-ir.

Focused native regressions cover field offsets (including loaded generic MIR),
unsigned NEON operations, relaxed anonymous and associated output bounds, imported
primitive types, directory iteration, symlink metadata, executable paths and sleep.
Manual Future polling, Pin receivers, pinned arrays and Option projection, waker
ownership, temporary lifetimes and in-place destruction also match rustc with
optimization disabled and enabled. Async blocks now use stable heap frames,
saved suspension states and the checked scope cleanup for each cancellation state.
Native comparisons cover nested awaits, borrowed outputs, loop resumption,
conditional and partial moves, pinned local addresses, output ownership and
cancellation before polling, at either await, and after completion. The round-robin
consumer polls pinned optional lanes and drops completed futures in place.
Incremental package tests also reuse a saved library exporting an async future,
including its private polling implementation, and rebuild it after a transitive
include changes. Futures remain conservatively !Send, !Sync and !Unpin.
The restoration goal remains open until the current complete graph builds and the
resulting compiler is exercised, followed by a complete stage-2 rebuild attempt.

Prerequisite validation on 2026-10-30 passed formatting, workspace clippy with
warnings denied, all 43 runner unit tests, the complete runner suite (464 targets,
zero failures), and workspace/example `fluxc check`. Runner evidence is in
`out/test-runs/1793335126135327000-63919/results.json`. That regression pass is
separate from the still-failing complete current-pin compiler audit above.

The trait opaque-return implementation additionally passes seven existing MIR
opaque/lifetime regression targets (16 tests), the scoped HIR snapshot, workspace
clippy and the new native/contract tests. These checks follow the 464-target pass;
the later complete run is recorded below.

After the async implementation, the complete runner covered 466 targets in
`out/test-runs/1793338201586143000-70593/results.json`: 465 passed and one had
an obsolete trait-profile rejection-reason assertion. That expectation now names
the method-generics gate reached by an admitted opaque return; its full 16-test
target passes on rerun. The unchanged HIR, type-checker and MIR native comparisons
passed, as did the complete runtime sweep. Formatting, workspace clippy, 43 runner
unit tests and workspace/example source checks pass. A subsequent source-owner
fix also passes a native comparison of async panic file, line and column, followed
by fresh async behavior/safety and incremental-package regressions. This evidence
still does not establish stage 1 or MIR admission for the complete current graph.

The atomic API tranche removes 116 type diagnostics, leaving 83 in the guarded
audit. No missing atomic-method diagnostics remain. Integer, boolean and thin
pointer CAS, updates and unsafe atomic views pass the new native comparison
with optimization off/on, including a four-thread halfword retry loop. The
existing byte atomic targets and pointer/32-bit/machine-word codegen targets
pass, as do the two bridge signature-rejection tests, formatting and workspace
clippy. The subsequent complete suite passed all 468 targets in
`out/test-runs/1793341745434010000-79353/results.json`.


The pointer/slice audit in `out/flux-compiler-readiness/78236` reports zero syntax,
zero HIR and 17 type diagnostics, with both source-integrity reports clean. It
covers address mapping that preserves slice lengths and object vtables, NonNull
copy/write/replace operations, byte offsets, checked array windows, mutable
remaining-array-iterator views, Result borrowing, string range replacement,
control-character classification, directory queries and array-to-set conversion.
Native comparisons and incremental saved-library address mapping pass in both
optimization modes. The complete 468-target pass above includes these changes.
Later unsized-library, pointer-equality and nested-lifetime changes need a new
complete-graph audit before they establish further progress toward stage 1.


The unsized-library follow-up `out/flux-compiler-readiness/41130` has zero syntax
and HIR diagnostics and three type diagnostics. Both source-integrity reports
are clean. Native tests now cover unsized ManuallyDrop, slice/object needs_drop,
wide-pointer equality including metadata, nonzero OR, nested generic callback
signatures and uniformly quantified nested-lifetime auto impls. The full run in
`out/test-runs/1793342606397171000-41152/results.json` passed 467 of 468 targets;
the sole failure expected obsolete wide-equality diagnostics. The scoped snapshot
was regenerated and reviewed, and its regression passes. Workspace clippy and
all 43 runner unit tests pass. The later key-view admission and signature-proof
budget fixes still need a complete current-graph audit.


The key-view audit `out/flux-compiler-readiness/4970` leaves one type diagnostic,
with zero syntax and HIR diagnostics and clean source-integrity reports. Raising
the bounded signature-proof recursion budget clears both proc-macro resolver
auto-trait errors; positive deep-wrapper and negative raw-pointer tests cover it.
The 469-target run in `out/test-runs/1793343433988242000-5122/results.json`
passes 468 targets. Its sole failure still expected owner type-outlives bounds
to be rejected. That expectation now exercises an unsupported tuple subject;
the complete 16-test trait-profile target passes. Native key-view methods,
borrowed-key defaults, unsafe-call checks, owner bounds and callback captures
pass off/on, with five invalid borrow/safety cases rejected by both compilers.
The subsequent callback-bound admission needs a fresh complete-graph audit.


The callback follow-up `out/flux-compiler-readiness/57214` reaches zero syntax,
HIR and type diagnostics. MIR checking reports four lifetime diagnostics:
flattened borrowed iterator outputs, an impl method's Self: 'static condition,
body trait-object defaults, and the proc-macro resolver's deep supertrait proof.
Both source-integrity reports are clean. The complete regression run in
`out/test-runs/1793343924402353000-57334/results.json` passes all 469 targets.
This clears the current-pin type barrier, but does not establish stage 1.

Focused fixes for those MIR cases pass the native key-view fixture with both
optimization modes and its borrow/safety rejection cases. They also pass nine
related MIR targets (20 tests) and ten region metadata unit tests. Stored
associated fields of opaque returns now retain their actual output region rows;
impl Self bounds use the header's type and lifetimes; omitted body object bounds
are inferred while aliases, defaults, explicit static bounds and static
supertraits retain their constraints. These changes still need a fresh guarded
complete-graph audit and complete regression pass.


The guarded current-pin audit `out/flux-compiler-readiness/66944` builds stage-1
Flux in both optimization modes. All syntax, HIR, type, MIR and code-generation
diagnostic reports are empty. Both generated executables pass help/version and
compile/run smoke comparisons against the rustc-built reference. The final source
and parent source integrity reports are clean. This is the first recorded full
stage-1 proof for Keel `103c23de480d76e02267c5a8d5a9d3505bc73231` without a
local backend override. The audit completed in 666.05 seconds.

Subsequent validation exposed repeated whole-declaration scans and field-path
expansion in outlives checking. Field expansion now uses the existing projection
closure, and the closure query visits only declarations reachable from its input
type. Leaf queries return their components directly. The complete HIR source
comparison and the 24 package-driver tests pass after those changes. Named
containing-type object defaults and alias defaults now retain explicit parameter
bounds; unsupported elided containing defaults reject explicitly. The complete
suite and complete compiler audit need rerunning on this follow-up before it
supersedes the stage-1 evidence above. Stage 2 remains untested.


The final guarded follow-up `out/flux-compiler-readiness/18888` passes stage 1
in both modes with all diagnostic reports empty and clean final source guards.
The additional optimized-program comparisons are recorded in
`out/stage1-extra-smoke/results.json`. The full suite was rerun separately from
the compiler audit and passes all 469 targets in
`out/test-runs/1793349006676445000-49634/results.json`. Formatting, workspace
clippy with warnings denied, 43 runner unit tests and workspace/example source
checks pass. Public Rustdoc generation completes with two pre-existing private
item-link warnings. An additional strict private-doc check exposes existing link
issues; it is not a passing validation claim.

Do not run this compiler audit alongside the full regression runner. Its Cargo
reference build uses the dev profile and replaces `target/debug/flux`, while
native regressions require the runner's test-profile compiler. Concurrent runs
caused timeout failures; the isolated full run above is the authoritative result.

The stage-2 command and result are saved in `out/bootstrap-stage2/command.json`
and `status.json`. The optimized stage-1 compiler was asked to run `cargo build`
for the complete `flux-cli` package with the locked offline graph. It exits 2
during preparation with `Cargo did not check target embedded_io` for
`embedded-io 0.4.0`. All 4,258 inventoried inputs remain unchanged before and
after the attempt. Postcard enables only `alloc`; its optional embedded-io edges
are still present in metadata. The Cargo source driver then traversed every
normal metadata edge and required an artifact for it. No stage-2 target artifact
was built by that attempt.

The subsequent Cargo driver fix selects optional edges from the checked
target's enabled features and the package feature definitions. `dep:name` and
`name/feature` activate the dependency; `name?/feature` does not. A two-crate
reproducer confirms that metadata retains the weak edge while Cargo check emits
no artifact for it. The native `cargo_host` regression covers weak forwarding,
nested and explicit activation, renamed libraries, and an optional target proc
macro also used by a build script. The updated driver prepares the repository
successfully with 72 source crates and 5 host proc macros; the plan is saved as
`out/optional-dependency-investigation/flux-plan.json`. This is preparation
evidence only. The stage-1 binaries from that audit predate this driver fix;
the fresh stage-1 build and stage-2 retry are recorded below.

## Optional-dependency fix verification

The fresh guarded audit in `out/flux-compiler-readiness/81431` builds stage 1
with optimization disabled and enabled in 256.55 seconds. Both executables pass
CLI and native smoke comparisons. All syntax, HIR, type, MIR and code-generation
diagnostic reports contain zero diagnostics. Both source-integrity reports are
empty.

Both stage-1 compilers also build and run the `cargo_host` dependency fixture
with optional dependencies disabled, enabled through a nested feature, enabled
through strong feature forwarding, and with an optional target proc macro
also used by a build script. All eight cases pass and produce the expected
native output. The results are saved in
`out/bootstrap-stage2-optional/stage1-cargo-fixture-results.json` and
`stage1-unoptimized-cargo-fixture-results.json`.

The full stage-2 retry is recorded in `out/bootstrap-stage2-optional/`.
`plan-comparison.json` confirms that the self-built and rustc-built drivers
produce identical preparation plans. `status.json` records the 1,800-second
timeout, no preparation blockers and no stage-2 executable. No target compiler
diagnostics were emitted before timeout. All 4,258 inventoried inputs remain
unchanged. A process observation at 29 minutes records approximately 12 GiB of
resident memory. This attempt verifies the optional-dependency fix through the
self-built driver, but does not establish stage-2 self-hosting.

The unchanged production fix also passed the complete 469-target regression
suite in `out/test-runs/1793350466552616000-33436/results.json`, workspace clippy
with warnings denied, formatting, 43 runner unit tests and workspace/example
source checks before this guarded retry.


## Profiling generated compilers

Set `FLUX_COMPILER_SAVE_OBJECTS=1` when running `flux_compiler_readiness` to save
`flux-unoptimized.o` and `flux-optimized.o` beside the generated executables.
The objects retain local function symbols that the linked executables omit.
On arm64 macOS, process samples can be mapped through those symbol offsets and
the executable's text-section address. Match the sampled executable's text
bytes to the saved object build before reusing an address map.

When comparing compiler speed, use identical source graphs and program
optimization options. Force `FLUX_BUILTINS_SOURCE=1` on both compilers to avoid
comparing staged built-ins against source-library compilation. An optimized
`flux build` takes debug assertions from the release profile, off by default
as in Cargo; build the rustc reference with the same profile settings. The generated compiler uses Flux's runtime libraries; that comparison
includes library implementation differences as well as generated instructions.

## Linux backend capability boundary

Keel `a078986c` supplies 512-bit vector IR and typed SHA-256 and VPCLMUL
operations. Flux supports the AVX-512F CRC folding subset, 128/256/512-bit
carryless multiplication and SHA-256 message/round instructions. Native Linux
comparisons cover the unchanged crc32fast 1.5.3 sources with its real build-script
cfgs, including `stable_vpclmulqdq`, in both optimization modes.

Runtime detection checks CPUID and OS-managed register state before enabling
AVX-512 paths. Other AVX-512 APIs remain outside the supported subset. The
Linux compiler dependency graph now builds through stage 2 with the library and
legacy const-argument support described above.
