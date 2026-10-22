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
packages; this is not the host-only compiler package count. The current
executable subset has no external dependencies.

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

Nested `cfg_attr` and item/body conditional pruning are supported; conditional
fields, variants and parameters remain unsupported. Other required constructs
include generated/included source,
procedural macros and the unsafe implementations in the arena/interner.
These need focused probes and Rust-compatible semantics; simply parsing
the workspace does not establish support.

Concrete later consumers include `AstNode<'t>`'s trait lifetime parameter,
the labeled block `'alias: { ... }` in type resolution. `repr(align(N))`
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
2. Compile the lexer and token input after defining how their two external
   dependencies are built. Compare token streams on real compiler sources.
3. Compile the parser and syntax-tree pipeline, then require its output on
   the workspace to agree with the rustc-built compiler.
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
with and without Flux optimization. Registry dependency resolution and the
remaining `memchr` language/library requirements are still outstanding.

Unchanged `memchr 2.8.3` sources still need `Send`/`Sync`, per-function
`target_feature`, `non_exhaustive`, aggregate
union fields and some remaining NEON APIs. Its original package
also needs feature selection and broader edition-2021 macro support. Source
probes use the file command's explicit edition-2024 boundary and do not
establish package compilation.

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
compound const expressions remain unsupported. Caller feature attributes remain
a next step. External C vector ABI is not
supported. Baseline `target_feature` predicates select NEON on AArch64 and
SSE/SSE2/FXSR on x86-64 independently of the compiler's own build flags.

The union front end parses contextual declarations and macro item fragments;
disabled declarations are pruned before semantic processing. All 45 unchanged
`memchr` Rust files parse and match the syntax grammar. A bounded executable
integer-field union subset supports overlapping storage, default/`repr(C)`
layout, Copy/Clone, safe field writes, unsafe reads/references, moves and
overlap checks. Union constant evaluation, generic and aggregate fields and
patterns remain unsupported. Actual `PrefilterKind` aggregate fields need
the next extension; parsing success alone does not establish their execution.
