# Compiling Flux with Flux

The first milestone is to compile and execute small, current compiler
modules with Flux, and compare them with rustc. This is not yet a build of
the whole compiler or a bootstrap test.

## Executable subset

On arm64 macOS, run:

```sh
just selfhost-subset
```

The runtime harness compiles these production sources, without rewriting
their algorithms:

- `crates/flux-difftest/src/rng.rs`: seeded random choices, weighted draws,
  recorded tapes and replay past the end of a tape.
- `crates/flux-mir/src/bitset.rs`: dataflow sets, mutation, cloning and
  union across partial and complete machine words.
- `crates/flux-mir/src/interval.rs`: region intervals, insertion in sorted
  order, coalescing, union and point lookup using `partition_point`.
- `crates/flux-syntax/src/syntax_kind.rs`: keyword classification and its
  constant perfect-hash table, punctuation, composite token slices and
  the exported `T!` macro, including `T![$]`.
- `crates/flux-syntax/src/parser/event.rs`: compact parser events,
  forward-parent indices, error storage, cloning and equality. Its original
  eight-byte layout assertion remains enabled.
- `crates/flux-difftest/src/generator/ty.rs`: integer boundaries and literal
  generation, deterministic character choices, recursive type names, cloning,
  equality and hashing.
- `crates/flux-syntax/src/tree.rs`: lossless tokens, node navigation, tree
  validation, expansion origins/context mapping, cloning and identity hashing.
  Its original sixteen-byte node layout assertion remains enabled.

The test embeds the source files as modules beside
`tests/run/support/compiler_internals.rs`. It compares output and exit
status against rustc with Flux optimization both disabled and enabled.
The interval checks also compare membership with a point-wise oracle.
Syntax-kind checks are appended inside its module from
`tests/run/support/compiler_syntax_kind.rs` so they can use its private
keyword spellings without copying the production table.
Test-only items are excluded by Flux's conditional item support, rather
than stripped from the test input. Only ordinary builds are covered;
this does not run the modules' `#[test]` functions under Flux.

Bundling avoids two separate build-system gaps: `#[path]` module loading
and loading separately compiled dependency crates. The normal workspace
suite includes this test; the dedicated recipe makes it quick to repeat
while expanding the subset.

On 2026-10-23 the seven-module test matched rustc in both Flux modes,
using the unchanged syntax-kind and event sources, including the explicit
local `'static` annotation and original event layout assertion.
With item configuration filtering,
the earlier robustness corpus built MIR
for 2,417 of 60,594 registry source files, compared with 1,467 before the
change on the same corpus. This measures individual files reaching the
MIR stage; it does not establish native compilation of those crates.

The event module's `Option<NonZeroU32>` field uses a four-byte zero niche,
preserving the unchanged event layout rather than replacing the production type.

## Lexer with real source dependencies

`mbx test --release -p flux-cli --test lexer_selfhost` compiles the unchanged
production lexer, syntax-kind and tree modules, with the production `Edition`
declaration, through an explicit library wrapper that excludes the typed AST.
The graph uses unchanged memchr sources through the edition-2024 feature-free
wrapper and the original unicode-ident package. Flux invokes no rustc during
its build; separate rustc builds provide the execution oracle.

The comparison checks exact token kinds, ranges, text, contextual flags and
errors on small valid and invalid inputs, 796 UTF-8 prefixes, and digests over
eight production files (172,994 bytes at this checkpoint). Both source editions
and both optimization modes agree. This extends native coverage to the actual
lexer without claiming that the complete syntax crate compiles. Its full
source graph still first rejects lifetime parameters on `AstNode<'t>`. The
named type arguments in `ast/ext.rs`, originally another blocker, are now
enforced at direct calls as described below.

## Parser over real lexer input

`mbx test --release -p flux-cli --test parser_selfhost` compiles the unchanged
production parser, its input/event types and complete grammar modules alongside
the actual lexer. An explicit wrapper retains `Input::from_lexed` and the
production Edition/TextRange declarations, while excluding tree construction
and the typed AST. It uses the same real dependency boundary as the lexer test.

At its initial checkpoint, rustc and Flux agreed with and without optimization
on 22 parser fixtures, 18,334 UTF-8 prefixes and 20 fragment cases spanning 14 fragment kinds.
The comparison checks events, errors, complete input consumption and replay of
202,928 forward-parent links. Fragment matching at each token position is also
covered. These counts describe the current fixtures; the test reads production
sources and parser fixtures as they evolve. No compiler repair was required.

## Lossless tree pipeline

`mbx test --release -p flux-cli --test tree_pipeline_selfhost` extends the
source graph through the unchanged `build.rs` and `tree.rs`, including both
`build_tree` and the owned-text build path. The library wrapper retains the
production parse entry points and declarations, excluding the typed AST and
its `source_file` accessor. All other production modules remain unchanged.

Flux and rustc agree without and with optimization on complete tree dumps,
diagnostics, token ranges, trivia attachment and navigation. Coverage includes
the parser fixtures and their UTF-8 prefixes, six workspace sources and 35
expansion-origin cases covering span merging, hygiene and missing origins.
At its initial checkpoint this produced identical 2,031,458-byte outputs.
No compiler repair was needed. This establishes native execution of the
untyped syntax pipeline, while the complete syntax crate still needs proper
trait lifetime support for its typed AST.

## Main gaps to the complete compiler

### Compiler input and crate builds

`flux build` currently compiles one root source file and its module tree
with Flux's own `core`, `alloc` and bounded `std` facade. The restricted
`flux package build/run` driver selects a standalone binary
and local path library dependencies using offline Cargo metadata; see
[its supported subset](PACKAGE_DRIVER.md).
It loads dependency sources as distinct crates in a shared item tree,
preserving per-crate aliases, privacy and type identity, and monomorphizes
the reachable program together. The unchanged compiler `rng.rs` is also
tested through this path-library route. It does not yet consume a general
Cargo resolve graph or rustc library metadata. Full compiler builds still
need registry/git dependencies, broader edition support, features, target
configuration and workspace support. Exported dependency macros now retain
definition-crate identity through aliases and reexports; general expansion
fixed points remain outside the current sequential item collector.
Separate compilation would additionally need library artifacts and serialized
metadata. Linking against a
rustc-built Rust crate is not a replacement for this work.

[The Cargo/Keel integration plan](CARGO_INTEGRATION.md) compares a real
Cargo compiler adapter, a `rustc_codegen_keel` backend, and a thin manifest
driver. It recommends reusing Cargo's package resolution and first building
one dependency-free binary through a small manifest driver, then loading
source dependency graphs. A Keel backend for rustc is a separate route
that retains rustc's frontend and metadata; it does not self-host Flux.

The dependencies themselves are part of the scope: `memchr` and
`unicode-ident` for syntax, `hashbrown` and `rustc-hash` for compiler data
structures, `clap`'s derives for the CLI, and Keel for code generation.
Each must either be supported or have a deliberately defined build
boundary. The initial offline Cargo audit found 58 external packages in
the workspace's normal/build dependency closure, including target-specific
packages; this is not the host-only compiler package count. The seven-module
bundled subset has no external dependencies; the additional lexer/parser/tree
source graphs compile actual memchr and unicode-ident implementations.

Keel generates selector tables in a build script and loads them through
`include!(concat!(env!("OUT_DIR"), ...))`. Supporting its build therefore
also requires build-script outputs and generated source. Sourcegen and
difftest are additional workspace targets, rather than prerequisites for
the first compiler executable.

### Language and semantic support

The syntax-kind milestone fixed the following blockers in its unchanged
production source:

- A matcher ending with a literal `$`, used by `T![$]`. This is now
  accepted and has runtime regression coverage.
- The explicit `'static` in a local type annotation in `composite_parts`
  is now enforced through MIR region constraints. Nested annotations,
  aliases, defaults, destructuring and later assignments have coverage.
- Returning match-selected promoted slices of fieldless enum values
  through `Some(parts)` previously failed with a temporary-reference error.
  Bug 017 is now fixed and covered by
  [runtime regressions](../tests/run/enum_array_promotion.rs). The production
  syntax-kind source is not rewritten to bypass its lifetime annotation.

Other named body lifetimes, lifetime-qualified function pointers and
associated projections, and annotations on uncaptured closure places
remain explicitly unsupported.

Direct function/method turbofish type arguments now preserve written named,
impl-owned and static regions through MIR call instantiation and enforce
well-formedness. This removes the five named-type-argument diagnostics from
unchanged `ast/ext.rs`; it does not yet unblock `AstNode<'t>`'s trait lifetimes.
Regression oracles cover unrelated caller lifetimes, reference escapes, nested
types and aliases, bounds and unused results. Named local annotations,
function-item values and closures remain outside this extension.

Trait lifetime support has two validated prerequisites: signature metadata now
retains trait-owner lifetimes, impl-header lifetime arguments, predicate region
tables and inherited method regions; impl method comparison substitutes the
owner regions through the impl header. Metadata and rustc-oracle tests cover
these mappings. Trait lifetime declarations remain rejected until
well-formedness, obligation selection, call-site substitution and inherited
trait/projection regions enforce their constraints. These prerequisites do not
yet compile the typed AST.

Nested `cfg_attr` and item/body conditional pruning are supported; conditional
fields, variants and parameters remain unsupported. Other required constructs
include generated/included source,
procedural macros and the unsafe implementations in the arena/interner.
These need focused probes and Rust-compatible semantics; simply parsing
the workspace does not establish support.

Concrete later consumers include `AstNode<'t>`'s trait lifetime parameter.
Labeled block expressions used by `'alias: { ... }` in type resolution now
support break values, fallthrough and nested exits with scope cleanup; targeted
initialization, move and borrow rejections agree with rustc. Whole type-checker
compilation still needs its source dependencies and runtime APIs.
`repr(align(N))`
now supports the arena's explicit alignment prerequisite, but this alone does
not establish arena compilation. The parser's `NonZeroU32` prerequisite now has
checked `new`/`get`, comparison and formatting support with the correct
zero niche. `Event` is eight bytes and `Option<NonZeroU32>` is four bytes;
the remaining NonZero family and APIs are not implied by this subset.

### Library and runtime

The compiler uses much more than its current `core`/`alloc` subset.
Examples include `Cell`/`RefCell` in the arena, synchronization and
atomics in the interner, hash tables throughout semantic analysis, and
filesystem, process and timing APIs in the driver and tools. The parser
also uses `std::thread::panicking` for its marker guard.

A small `std` facade now reexports facilities already present in `core`
and `alloc`, preserving their definition identities; see
[its API boundary](../library/std/README.md). Its `thread::panicking` is
always false in the supported abort-only runtime. The facade does not provide
the missing runtime facilities above. Other
observed API gaps include Unicode case conversion and
`String::from_utf8_lossy`. String-slice `join` now accepts `Borrow<str>`,
including `String` and user-defined borrowers. Boxed slices and strings
support cloning, and boxed slices support iterator collection. Pointer
hashing currently accepts thin pointers only. Passing `&mut closure` as an
`FnMut` argument also remains unsupported; the new partition-point tests pass closures by
value, including closures that borrow mutable state.

A deliberate standard-library/runtime plan is needed before the whole
workspace is executable. Small dependency-free modules expose core and
alloc gaps first, without requiring an operating-system API all at once.

## Next milestones

1. Keep the executable source subset passing as its source evolves.
2. Keep the lexer source-dependency comparison passing on real compiler inputs.
3. Keep the complete untyped lexer/parser/tree source graph passing on compiler
   inputs. Add trait lifetime support to include the complete typed AST
   and compile the whole syntax crate through source dependencies.
4. Expand into arena/interner and semantic analysis, while adding crate
   builds and the required library/runtime facilities.

The backend already emits arm64 macOS and x86-64 Linux objects, but the
current executable link path is implemented only for arm64 macOS. A
rustc-built Keel boundary may be useful during bootstrapping; it still
leaves part of the compiler built by rustc. Compiling the whole compiler
with Flux ultimately includes Keel and its dependency/build-script closure.

The final bootstrap criterion is stronger than a successful build: a
Flux-built Flux must compile and run the regression suite and then build
the compiler again with equivalent behavior.

The original cached `unicode-ident 1.0.27` manifest and sources now compile
as a local path dependency under the bounded edition-2021 implementation.
Classification counts and a digest across every Unicode scalar match rustc
with and without Flux optimization. Registry dependency resolution remains
outstanding.

Unchanged `memchr 2.8.3` sources now lower to HIR without diagnostics under the
file command's edition-2024 boundary, and the complete source root now passes
type/MIR checking. Concrete Copy union fields, function-pointer Clone and
Result `unwrap_unchecked` remove the last diagnostics from that probe.
Thin pointer comparisons and bounded unsigned unaligned
reads now have runtime coverage. Cfg pruning
now precedes block-tail selection, removing the earlier unit/value mismatches.
A package regression now compiles those unchanged sources as a path dependency
through an explicit edition-2024, feature-free wrapper. Across 32,640 cases,
rustc and Flux executables agree both without and with optimization: byte
search variants, counts, substring search, cloned finders and forward/reverse
iterators are exercised at different lengths and offsets. This is native
compilation of a real dependency's sources, without rustc in the Flux build.
The original manifest still needs feature selection, optional registry
dependencies and broader edition-2021 semantics; it is not yet supported.

Struct/enum `non_exhaustive` attributes enforce external construction and
pattern restrictions through source dependencies. Variant-level attributes
remain unsupported.

Bounded Send/Sync checking now supports structural obligations, generic bounds,
reference rules and explicit unsafe implementations, including memchr's Iter
declaration. Shared statics use those same Sync rules. Lifetime-specialized
marker implementations and general custom auto traits remain unsupported.

Macro-generated block imports now share ordinary import scopes, hygiene and
privacy checks. Other generated local items remain unsupported, as do body
macro calls whose selected definition changes after sequential collection.

The NEON foundation now supplies distinct fixed-layout types, unaligned
loads, splat, byte comparison, bitwise AND/OR, reinterpretation and checked
const-generic lane extraction through packed Keel operations. Calls require
unsafe code in the current subset. Public callable wrappers also support
unsafe function pointers; they do not expose intrinsic stub bodies to indirect
calls. Narrowing shift and pairwise unsigned maximum are also supported:
narrowing checks `1..=8` and writes eight bytes through scalar operations;
pairwise maximum combines scalarized shuffles with packed comparison/selection.
Native `shrn`/`umaxp` selection is not implemented. Legacy trailing const
arguments now work for the two lane APIs and narrowing shifts in direct calls;
compound const expressions remain unsupported. Explicit unsafe functions with
bodies now accept baseline `target_feature(enable = "neon")` on AArch64,
including methods and const functions. Safe functions and other features remain
unsupported. External C vector ABI is not
supported. Baseline `target_feature` predicates select NEON on AArch64 and
SSE/SSE2/FXSR on x86-64 independently of the compiler's own build flags.

The union front end parses contextual declarations and macro item fragments;
disabled declarations are pruned before semantic processing. All 45 unchanged
`memchr` Rust files parse and match the syntax grammar. A bounded executable
concrete-Copy union subset supports overlapping storage, default/`repr(C)`
layout, Copy/Clone, safe field writes, unsafe reads/references, moves and
overlap checks, including nested projections through different union fields.
Accepted storage recursively includes integers, bool, unit, function pointers,
arrays, structs, enum payloads and nested unions, including the array-backed
NEON wrappers and actual SearcherKind fields. Union constant evaluation,
generic union declarations, stored references/raw pointers and patterns remain
unsupported. Copy requirements and the region-free storage restriction remain
checked recursively.
