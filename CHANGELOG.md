# Changelog

## Unreleased

### Syntax

- Add a lexer for Rust edition 2024 tokens: nested comments, raw
  identifiers, lifetimes, char, byte, string, raw string, byte string and C
  string literals, and integer and float literals with suffixes.
- Add an error-recovering parser for items, generics, types, patterns,
  expressions and statements in the [supported subset](docs/LANGUAGE.md),
  producing lossless syntax trees.
- Add `flux lex`, `flux parse` and `flux check`.
- Add a typed AST (`flux_syntax::ast`) generated from `flux.ungram` by the
  new `flux-sourcegen` crate, whose tests check that parsed trees match
  the grammar.
- Add `flux-hir`: the item tree and HIR lowering of function bodies,
  with names resolved, and `flux hir` to print it.
- Add `flux-ty`: type inference and checking for HIR bodies, with
  diagnostics matching rustc's for the supported subset.
- Add `flux-mir`: MIR built from typed HIR, with wrapping arithmetic,
  masked shifts and division checks explicit, and checks for
  uninitialized and reassigned locals. `flux mir` prints it.
- Add `flux-codegen`: MIR to keel IR, native objects and signed arm64
  macOS executables, all in-process through keel. `flux build` and
  `flux run` compile programs, and `tests/run` checks them against rustc.
- Support C strings (`c"..."`, `cr"..."`) with `CStr::as_ptr`, raw pointer
  types, and calls to C-variadic functions such as `printf`, with
  rustc's rules for variadic arguments.
- Intern types: `Ty` is a 4-byte handle, so types can contain types.
- Support structs (named-field, tuple and unit) and tuples: struct
  expressions with `..base`, field access and assignment, and tuple and
  struct patterns in `let` and parameters. MIR reads values from places
  and checks moves, including partial moves, with rustc's messages
  (E0382, E0381, E0594). Code generation splits aggregates into scalars
  and returns them through `sret`.
- Support inherent `impl` blocks with associated functions and methods,
  `Self`, references (`&T`, `&mut T`, `&x`, `*r`) with auto-ref and
  auto-deref, and a borrow checker with rustc's non-lexical lifetimes
  and messages for the references flux can check without lifetimes.
  Borrowed locals live in stack slots.
- Support modules (inline and in files), `use` imports (groups, renames,
  globs, re-exports), visibility and privacy, constants and `const fn`
  with compile-time evaluation, and rustc's `arithmetic_overflow` and
  `unconditional_panic` lints. Programs are read as a set of files.
- Add flux's own `core` library, written in flux and built into the
  compiler, with the integer types' constants and methods.
- Support generics: type parameters on functions, structs and inherent
  `impl` blocks, generic arguments in types and paths (turbofish), `_`
  in types inside bodies, and blocks for different instances of a
  struct with the same method names. Generic functions are checked once
  and monomorphized in code generation. Diagnostics follow rustc's
  (E0107, E0109, E0282, E0034, E0392, E0207, E0592, ...), and expected
  types flow into calls and struct expressions as in rustc. `core` gains
  `mem::drop` (in the prelude) and `convert::identity`.
- Support traits: required and provided methods, generic traits,
  supertraits, `impl` blocks of traits for any type (blanket ones
  included), trait bounds and `where` clauses, method calls through
  bounds and traits in scope, and `T::f`, `Tr::f` and `<T as Tr>::f`
  paths. Bounds are checked at calls with rustc's spans (E0277), and
  `impl` blocks against their traits and each other (E0046, E0407,
  E0053, E0050, E0185, E0186, E0049, E0119, E0199, E0200, E0391).
  Calls of trait methods are resolved to `impl` blocks when generic
  code is monomorphized. `core` gains `marker::Sized`.
- Support associated types (in traits, `impl` blocks, bounds' bindings
  and paths: `T::Out`, `<T as Tr>::Out`), defaults for trait parameters,
  and operators on user types through `core::ops` and
  `core::cmp::PartialEq`, which `core` implements for the integers and
  `bool`. Traits may carry `#[diagnostic::on_unimplemented]` messages,
  as `core`'s operator traits do. `core`'s bodies are lowered and
  type-checked only where the program reaches them, and its files are
  parsed in parallel.
- Support `Copy` and `Clone` (`core::marker`, `core::clone`, in the
  prelude): values of types that implement `Copy` are copied, others
  moved, and E0204 checks `Copy` impls. `#[derive(Clone, Copy,
  PartialEq, Eq)]` on structs implements them as rustc does.
- Support `Drop` (`core::ops`, in the prelude): bindings, parameters and
  temporaries are dropped where rustc drops them, in rustc's order, with
  edition 2024's rules for tail expressions and rustc's lifetime
  extension of temporaries a `let` borrows; values moved out on some
  paths only are dropped through drop flags, and partially moved ones
  drop their remaining fields. Drop glue is built when generic code is
  monomorphized. E0040, E0120, E0184, E0366, E0367, E0493 and E0509
  are reported as rustc does.
- Give macro calls a node kind per position: `MACRO_CALL` (items),
  `MACRO_EXPR`, `MACRO_TYPE` and `MACRO_PAT`.

### Performance

- Find the loans live at each point from the few locals that hold each
  one, rather than from every local: borrow checking takes a fifth of
  the time it did on generic code.

- Build `core`'s front end once per process: its parsed files, its item
  tree with names resolved, its signatures, its constants' values and
  its bodies as they are reached. A program's item tree continues
  `core`'s, sharing its items through the new `LayeredArena`, and its
  signatures are resolved after `core`'s. Lowering and type-checking an
  empty program in a process that has built `core` takes about 35 µs
  instead of 1.1 ms; a single `flux build` is unchanged.
- Read type kinds without a lock, from an append-only table of
  write-once slots, and hash types with FxHash: type checking `core` and
  a program takes about a third less time.

- Build flat syntax trees (preorder node arrays over the lexer's token
  arrays) instead of rowan green trees: 5.5 ns/token to build instead of
  24.4, and about 210 MiB/s for whole-file parsing on one thread instead
  of 100. The trees are identical to the rowan ones across the corpus.

- Lex bytes instead of characters, skipping comments and strings with
  `memchr` (8.9 to 6.0 ns/token), and shrink parser events from 32 to 8
  bytes (5.5 to 4.6 ns/token).
- Add `flux-intern`: a global, sharded `Symbol` interner with per-thread
  caches, lock-free resolution and predefined keywords and names.
- Add `flux-arena`: `DroplessArena` for bump allocation, and `Arena<T>`,
  `Idx<T>` and `ArenaMap` for index-based IRs.
- Add `just bench` and `just miri`.
