# Fluxlib separate compilation

Status: implementation in progress on `feat/flib`. `flux build --crate-type lib`
writes checked metadata and one native object. `--metadata-only` omits the object.
`flux build --extern alias=path.flib` compiles against decoded dependencies without
reading their sources. Package builds write libraries under `target/flux/deps`
and reuse exact input snapshots. `flux package build --lib` writes the root
library there too. Proc-macro exports retain checked host descriptors and use
the existing expansion protocol after loading. Trusted built-in persistence is
the remaining gate; the source-graph path remains the comparison baseline.

A `.flib` (Fluxlib) contains one checked Flux crate. Loading it supplies the
semantic information needed by downstream crates without parsing, lowering or
type-checking the dependency again. This is a private Flux format. It provides
no rustc metadata, Rust ABI or artifact interoperability.

## Container and wire format

Use a regular `ar` archive, with short member names and deterministic headers
(zero timestamps and owner IDs, fixed modes). Do not use thin archives or
external member paths. Initially it contains only `flux.meta`. Separate code
generation adds exactly one `flux.o`, a Keel-produced native object containing
the crate's non-generic definitions and any instances needed by those definitions.
The loader extracts object bytes explicitly; the archive needs no native symbol
index. Reject duplicate members and unsupported member kinds.

`flux.meta` begins with eight magic bytes `FLUXMETA`, a little-endian `u32`
format version, and a length-prefixed compiler build identity. A section directory
follows, with numeric section tags and checked `u64` offsets and lengths relative
to the metadata member. A trailing SHA-256 checksum covers the header, directory
and section bytes; framing readers detect damaged payloads before freshness or
semantic decoding. Version 1 requires every defined section; empty sections
have zero records. Reject unknown versions, unknown section tags, duplicates,
overlapping ranges and trailing payload bytes. Container changes increment the
format version; link-record changes increment the linkage ABI. Compiler identity
rejects metadata from other IR builds. There is no migration or best-effort decode.

Use explicit numeric variant tags, little-endian fixed-width integers, `u32`
record counts and byte lengths, UTF-8 strings, and canonical record ordering.
Never serialize Rust enum discriminants, memory layouts, pointers, `usize`,
interner handles or hash-map iteration order. Structural type records may refer
to an artifact-local type table to share repeated subtrees. The decoder checks
bounds, tags, UTF-8, arithmetic overflow, nesting depth and allocation budgets
before allocating or indexing. Malformed input returns an error, never a panic.
Each string/vector/table record is limited to 1,000,000 bytes or elements;
metadata is limited to 512 MiB. Encoding and decoding enforce the same record
limit. Packed references allow at most 256 crate slots, including the crate itself.
Compression, lazy decoding and mmap are deferred.

## Metadata sections

| Section | Required information |
| --- | --- |
| Identity | Crate name and package/source identity, edition, compilation configuration, crate hash, dependency hashes and aliases, compiler identity, target and artifact capability flags |
| Strings and sources | Symbol spellings; diagnostic file paths, source text and byte ranges; expansion origins and file relationships |
| Items and resolution | Crate-local modules, all item kinds and owners, fields and variants, visibility, generics, paths, attributes that affect semantics, resolved namespaces and re-exports |
| Macros | Exported declarative token trees, token spacing, spans, definition edition, defining crate and hygiene context graph; proc-macro export descriptors |
| Signatures | Function and field types, trait definitions and defaults, impl headers, predicates, projections and associated types, constant types, opaque bounds and hidden types |
| Regions | `RegionSigs`, binders, outlives requirements, region rows and their ordering, parameter variances and closure requirements |
| Constants | Evaluated values, discriminants, array lengths, allocations and relocations; generic constant recipes |
| Bodies | Checked, generic MIR templates, inline and const function MIR, constant initializers, closure bodies, promotions and source maps |
| Link requirements | Defined and imported stable symbols, native libraries, object target and ABI settings |

Encode the whole crate's semantic item slice initially, including private items.
Public signatures, hidden types, macros and MIR can reference private definitions.
Include all impl headers, not only impls reached by a particular consumer, so
coherence and selection have the same inputs as whole-graph compilation.
Rebuild derived indexes such as impl lookup and associated-item lookup from the
loaded records. Do not persist solver caches, inference state, locks or arenas.
`TyKind::Infer`, error types and bodies containing compiler bugs cannot be written
as successful artifacts. Hidden opaque types remain inaccessible to ordinary
name lookup but available for auto-trait queries and code generation.

MIR templates precede instance substitution (`flux_mir::mono::subst_body`).
Store all body-local IDs and the supporting closure/callable recipes, region data,
constant allocations and source mappings they reference. Preserve the phase at
which drops and intrinsics are expanded. A generic or const body must never
require dependency HIR inference as a fallback. Include the transitive body
requirements of serialized templates, including private const helpers.

The metadata-only checking milestone can omit non-generic runtime MIR. It must
reject executable builds that need absent code. An intermediate executable
comparison can retain all MIR until the object milestone supplies that code.
An inline body does not permit dropping its out-of-line definition when an
address or a call still requires it.

Store proc-macro names, kinds and host artifact identities, not loaded process
handles or executable bytes masquerading as target code. Descriptors identify
canonical host executable/dylib paths, their SHA-256 contents, platform,
protocol and export selector. Loading checks the actual host export catalog;
expansion rechecks its files. Each compilation graph pools restored host state,
while invocation crates and independent graphs remain isolated. Attribute,
derive and function exports keep their kind and helper declarations. The host
implementation source is never read while restoring a descriptor.

Host macros can observe undeclared state, so packages containing them decline
freshness reuse. Crate identity includes their actual output tokens and hygiene,
including body-position outputs, so a changed expansion invalidates downstream
artifacts even when source and host executable contents stay the same.

## Identity and freshness

Use a specified deterministic digest (SHA-256) over canonical bytes, rather than
Rust's process or implementation-dependent hashers. The crate hash covers the
package/source identity, target kind, edition, cfg/features, semantic and codegen
options, source contents and logical paths, compiler identity and exact dependency
hashes. Include macro-host inputs and other tracked environment/generated inputs
when supported. HIR records the actual paths and SHA-256 contents read by
`include!`, `include_str!` and `include_bytes!`, and the observed values or absence
of variables read by `env!` and `option_env!`. These observations enter crate
identity. Freshness also checks symlink targets and absent module alternatives;
modification times alone never establish freshness. Dependency aliases belong to the consuming crate's input.
Package compilation records the canonical manifest path, package name and
declared version. Distinct package versions or source identities remain distinct
even when their crate names and physical source files match. Do not include output paths or archive timestamps.

The compiler identity must cover compiler sources, embedded library contents,
the pinned Keel revision and build configuration that changes compilation.
The implementation hashes the running compiler executable with SHA-256. This
covers linked compiler/backend code and embedded libraries, including dirty
builds; the executable's installation path is not hashed. If the executable
cannot be read, artifact creation/loading fails rather than guessing identity.
Staged-suite lookup will record device, inode, size, modification and change
times (including nanoseconds) beside the hash. A complete stat match reuses the
recorded identity; every mismatch takes the full content-hash path. A copied
executable can therefore accept the same suite, while a dirty rebuild rejects it.
A release string or Git HEAD alone is insufficient. The object capability also checks target triple,
ABI/layout revision, panic strategy and codegen settings.

The loader validates the header, expected crate identity and dependency hashes
before committing records. A check-only artifact is never accepted as linkable. Link requirements record
an object checksum alongside the target and ABI revision; loading rejects a
damaged or substituted object.
A mismatched version, build, target or dependency yields a short error naming the
artifact and requiring a rebuild. Explicit artifact loading does not silently
fall back to dependency source. Package freshness separately compares recorded
inputs with current files; an internally valid artifact can still be stale.
Hashes detect mismatches, not authenticity.

## Remapping and loading

Artifact item references are `(crate slot, item kind, local index)`. Slot zero
means this crate; other slots name exact dependency hashes. Assign local indices
in deterministic arena order, using explicit ownership maps rather than assuming
every crate occupies a contiguous range in every process table. Dependencies are
loaded first and deduplicated by identity and hash; aliases do not duplicate them.

Loading has two passes. First validate into temporary wire records and reserve
all destination IDs for modules, functions, structs, traits, impls, constants,
aliases, macros and files. Then fill records through per-kind remapping tables.
A contiguous append can implement a mapping as an offset, but references to
other crates always go through their maps. Check kind and index on every edge.
Reserve all IDs before filling to handle mutually referring items.

Re-intern strings by spelling and types by their structural `TyKind`, rewriting
all embedded item references first. Preserve parameter and binder indices as
local indices, not arena offsets. Rewrite `CrateId`, `FileId`, restricted
visibility modules, closure owners, macro definition crates (`$crate`), hygiene
contexts, constant relocations and MIR item references too. Source/node keys
need explicit artifact-local maps; raw `NodeId` values cannot identify unrelated
syntax trees. Included-source coordinates are encoded as file identities plus
relative byte offsets and receive fresh coordinate intervals on load. Call-site
positions use the same mapping. Embedded source is for diagnostics, never a
source re-check path.

Append the remapped crate to `ItemTree` and its entries to `Signatures`; install
its def-map scopes and merge derived trait/impl indexes. Downstream declarations
still undergo coherence, privacy and well-formedness checks against the loaded
interfaces. Mark loaded bodies as checked and route mono/const queries directly
to decoded MIR. Build temporary state before publishing so a failed load cannot
leave a partial crate visible. Keep privileged embedded-library identity under
compiler control; an input artifact cannot grant itself `is_core` privileges.

## Symbols and linkage

The existing legacy-style mangler is a starting spelling only. Its instance
hash currently includes arena indices for some types. Replace those inputs with
stable crate identity, item kind/local identity and structural generic arguments.
Every non-generic definition has one owning object and one strong definition.
All cross-object call conventions must be fixed independently of whole-program
address-taken analysis; exported functions need a consistent reified convention
or explicit shims recorded in metadata.

Use content hashes for immutable data and vtables. Hash target/layout, alignment,
bytes and relocations expressed as stable symbols plus addends. Vtable keys
include the structural object/concrete types and ordered method/supertrait
entries. Use distinct symbol domains for strings, promotions and vtables.
Mutable statics and identity-sensitive allocations use their owning crate/item
identity; equal initial bytes must not merge their addresses. Recursive allocation
graphs need canonical allocation identities rather than recursive byte hashing.

Use hidden `weak_odr` definitions for duplicate generic instances and their
shims, with identical stable symbols, ABI and bodies in every object. The pinned
Keel revision `e4c72445` recognizes weak ODR symbols when selecting AArch64 address
relocations and resolves each weak copy's unwind records to that object's body.

Hidden definitions are private externs that resolve and coalesce across objects
within one executable. Fluxlib has no dynamic-export or interposition contract;
source visibility does not imply a dynamic symbol export. Imported C declarations
keep their external linkage. Hidden visibility is an image-local linkage policy,
not a workaround for the older Keel address-relocation bug.

Hidden strong definitions set `noninterposable`, so Keel's Full tier can inline
direct calls within their owning object while retaining the exported definition.
Keel `e4c72445` still rejects `weak_odr` bodies for inlining. Generic instances
retain shared linkage and address identity despite that optimization restriction.
Separate objects also prevent inlining non-generic functions across crates, and
use the fixed reified calling convention rather than the whole-program fast
convention. The tree pipeline fixture currently runs slower through separate
objects. Cross-crate inlining and improvements to Keel's weak ODR optimization
and fixed calling conventions remain follow-up work.

Native regressions verify one shared generic definition and equal function
addresses, data relocations, vtable calls and optimized/unoptimized output on
arm64 macOS. Immutable local data can use object-local copies; exported shared
data needs the tested coalescing rule.
The executable alone owns the C `main` wrapper and embedded runtime definitions.
Link each dependency object once, in addition to downstream monomorphizations.

## Delivery and acceptance

1. Commit this design and link it from the Cargo integration later gates.
2. Add the archive and handwritten binary codec, then structural types,
   predicates, paths and signatures. Test round trips, invalid tags, truncation,
   version/build mismatches and decoding after unrelated interner allocations.
3. Load one user crate into a differently populated graph. Compare downstream
   diagnostics with whole-graph checking, including aliases, re-exports, private
   fields, macros, constants, generics, regions and opaque auto-trait behavior.
   Run with dependency source unavailable and prove its check path is not called.
4. Add stable symbols, one object per crate and linking. Compare output and exit
   status with whole-graph Flux and rustc, optimized and unoptimized. Exercise
   repeated generic instantiation, statics, vtables and same-named crates.
5. Make `flux package build` write and reuse one `.flib` per library. Write
   atomically and test invalidation through transitive edits and configuration.
6. Persist embedded libraries only after `perf/library-cache` lands on `main`.

Keep codec primitives small and compatible with Flux's own libraries. Place
semantic codecs beside the IR they encode, with archive/driver assembly above
those crates; add no serde dependency or dependency from type IR to MIR/codegen.
If a shared codec crate becomes necessary, update the frozen compiler graph and
HIR/type/MIR self-host harness graphs together. Run the HIR and type self-host
targets when changing that dependency graph.

Each milestone gets its own local Conventional Commit and scoped validation.
No Cargo compiler adapter, general resolver expansion, cross-version artifact
compatibility, arbitrary proc-macro loading, cross-target linking or built-in
library persistence is established by the first milestones.

## Loaded-state integration

`flux_hir::lower_source_on` appends and lowers only the source crate above an
already loaded item tree. `flux_ty::flib::LoadedTypes` captures the checked
signature tables and source arena boundary; `signatures_from` resolves only
new declarations. `flux_mir::imported::ImportedMir` supplies checked MIR without
requiring imported HIR or inference records.

`ImportedMir::required_functions` produces one root bundle from imported calls,
vtables, promotions, constant allocations and the concrete instances required by
dependency objects. Function roots feed type checking and MIR construction;
concrete instance roots retain generic arguments for codegen. This matters even for an unused emitted library function: its object
may still contain references to embedded runtime functions. Lookup through the
provider decides whether a visited body is already checked.

Linkage ABI 2 records concrete instance requirements from the producing object's
actual monomorphization traversal, before imported definitions are excluded from
emission. Retained but unused generic MIR is not another source of object roots.
A function ID alone cannot describe a generic built-in call required only by an
upstream object. ABI 1 is rejected before reading the instance stream, with a
request to rebuild the artifact.

Symbolic exported templates use structural substitution and conservative local
implementation reachability. Concrete monomorphization still normalizes types
and resolves trait instances. Private helpers reached through fixed associated
type projections and opaque iterator returns have native boundary regressions.

Runtime `TypeId` uses 128 bits of a structural SHA-256 fingerprint after the
solver's existing normalization and binder canonicalization. Definition keys
contain crate identity, definition paths and disambiguators, with included
spans converted to file-relative positions. A collision between different
canonical keys is an internal error. Independent signature forks have separate
identity caches; clones inside one compilation share the cache.

## Built-in suite plan

After the user/provider/package gates pass, an explicit `flux build-builtins`
staging command will write `flux-builtins.flib` beside the built compiler under
`target/<profile>`. There is no `build.rs` and no first-run creation. The staged
suite will contain the compiler-owned `core`, `alloc` and `std` prefix in its
original item/file order, including diagnostic paths, include coordinate ranges,
roles and privileged lookup tables. User artifacts cannot enable this trusted
loader. The source path and suite path use the same suite/core-role identity.

CLI startup will select a matching suite or source mode for the whole invocation.
Missing and build-mismatched suites use source mode; a corrupt matching suite
will report its path and request `flux build-builtins` again. Staging uses an
atomic replacement in the same directory. A source override keeps native source
coverage in the runner. In-process tests retain the per-process library cache.

Measure native object size, link time and the run corpus with a complete suite
object versus reached-function emission from suite MIR. Record the object
optimization tier explicitly, and do not change debug/release library semantics
by accident. Preserve shared generic definitions and their image-local visibility when
comparing these strategies.
