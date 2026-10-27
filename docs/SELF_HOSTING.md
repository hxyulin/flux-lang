# Compiling Flux with Flux

Flux compiles and executes several unchanged compiler crates and dependencies
through explicit source graphs. These comparisons do not yet build the whole
compiler or establish a bootstrap. Package-manager work is deferred.

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
| `smallvec_readiness` | Unchanged smallvec 1.16.3 in its original edition 2018, using the workspace's resolved empty feature set. |
| `hir_selfhost` | Complete unchanged HIR with arena, interner, syntax and dependency sources, original editions and resolved library features. |
| `ty_selfhost` | Complete unchanged type checker and its nine-library source graph, with public inference and signature consumers. |
| `mir_selfhost` | Complete unchanged MIR crate and its eleven-library source graph, with public MIR building, borrow checking and constant-evaluation consumers. |
| `keel_asm_ir_readiness` | Complete pinned Keel assembly-IR source with original smallvec sources, structured assembly consumers and byte guards. |
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
cargo test -p flux-cli --test hir_selfhost
cargo test -p flux-cli --test ty_selfhost
cargo test -p flux-cli --test mir_selfhost
cargo test -p flux-cli --test keel_asm_ir_readiness
cargo test -p flux-cli --test keel_ir_readiness
cargo test -p flux-cli --test keel_float_math
cargo test -p flux-cli --test bumpalo_readiness
cargo test -p flux-cli --test equivalent_readiness
cargo test -p flux-cli --test crc32fast_readiness
```

The typed-AST consumer exercises 135 generated node interfaces, 535 accessors
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
twelve separately rustc-checked cases. It preserves original editions, resolved
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
  calls; complete assembler native execution remains blocked below.
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

The complete HIR, type-checker and MIR source graphs have guarded native
comparison harnesses. Keel IR, assembly IR, bumpalo and the actual Keel
floating-point constant folder also have native comparisons. These milestones
do not yet build the complete compiler executable. The assembler and its
object-file dependencies are the next source graph. CLI derives and Keel's
dependencies/generated outputs also belong to the
complete source closure. Sourcegen and difftest are additional workspace targets,
not prerequisites for the first compiler executable.

The latest assembler audit has zero syntax diagnostics and 80 HIR diagnostics,
down from 111 after trait `Self: 'static` bounds, older-edition opaque captures
and CRC support. Both source-integrity guards pass. All remaining diagnostics
are 70 GAT declarations and 10 associated `where` clauses. This graph still
stops before type checking and native codegen; see
the [readiness report](../crates/flux-cli/tests/fixtures/keel_asm/READINESS.md).

Current bounded language implementations still reject unsupported shapes.
The latest function-pointer review also found two unresolved correctness issues:
object lifetime defaults cross intervening container types incorrectly, and
implied input bounds permit some output-lifetime conversions that rustc rejects.
These findings remain open despite the existing focused oracle coverage.
Examples include unproven zero-sized transparent marker fields, broader dynamic
callable lifetime forms, generic associated-constant promotion, raw-slice construction
in constant evaluation and generic unions. The dynamic callback extension
currently covers direct elided references to concrete region-free inputs,
region-free outputs and closure-source coercions. This is enough for the
exercised HashTable graph, not every Rust trait object.

Virtual-source inclusion and include calls first resolved after body-item
collection remain explicitly rejected. Const memory interpretation is typed and
bounded; it is not a general byte-memory evaluator. Scoped Rust thread creation is supplied separately from TLS. Unscoped thread
creation and unwinding remain outside the runtime subset.

`flux build` compiles one root and its modules with embedded core, alloc and
bounded std. The separate restricted [package driver](PACKAGE_DRIVER.md) accepts
some offline local path graphs. Expanding registry/git resolution, feature
resolution, package artifacts and serialized metadata is deferred. Explicit
crate configuration and source dependency inputs can be extended independently
of that driver. Linking rustc-built Rust libraries is not a substitute for
compiling the required sources.

The backend emits arm64 macOS and x86-64 Linux objects; executable linking is
currently implemented for arm64 macOS. A rustc-built backend can help diagnose
frontend milestones, but leaves part of the compiler built by rustc. The final
source closure includes Keel and its required build outputs.

## Bootstrap criterion

The remaining sequence is to compile and execute the semantic compiler crates,
then the complete compiler and backend from source. A Flux-built Flux must
compile and run the regression suite and build the compiler again with
equivalent behavior. Component comparisons establish intermediate evidence;
none is a claim that this final criterion has been met.
