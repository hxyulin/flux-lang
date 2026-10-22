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
- Support bounds on structs, in their parameter lists and `where`
  clauses: fields may name the bounds' associated types, and as in
  rustc every type that names a struct must meet its bounds, in
  signatures, `impl` headers, annotations, paths, constructors and struct
  expressions (E0277, E0271), at rustc's positions. Derives keep a
  struct's bounds, and `Drop` impls may repeat them.
- Support associated constants in traits, with or without defaults, in
  their `impl` blocks and in generic inherent blocks, through `T::N`,
  `S::N`, `Self::N` and `<T as Tr>::N`. A constant whose value depends
  on generic arguments is evaluated for the instances that use it, so
  its errors are reported by `flux build`, as rustc reports them only in
  full builds. E0046, E0107, E0201, E0220, E0323, E0326, E0438 and
  E0790 are reported as rustc does.
- Build instances from `main`, as rustc does: generic code that only
  unused functions call is not instantiated.
- Support enums: unit, tuple and named-field variants, generic enums,
  discriminants and casts of enums without fields, variants through
  their enum (`E::A`, `Self::A`) and imports (`use E::*;`), and
  `#[derive]` on enums. Support `match` with guards, `if let`, `let`
  chains, `while let` and `let`-`else`, `if let` guards, and patterns
  with literals,
  ranges, or-patterns, `@`, `ref`, `ref mut` and `&`, with rustc's
  default binding modes and edition 2024's rules for them. `match`es
  must be exhaustive and `let` patterns irrefutable (E0004, E0005),
  reported with rustc's witnesses, integer ranges included. MIR tests
  what each pattern looks at and moves into bindings once an arm and
  its guard match; moves out of variants are tracked and dropped as in
  rustc. `core` gains `Option` and `Result` (in the prelude) with their
  methods that need no closures. E0023, E0026, E0027, E0029, E0030,
  E0081, E0164, E0370, E0408, E0409, E0416, E0449, E0507, E0510, E0530,
  E0532, E0533, E0559, E0574, E0579, E0594, E0599, E0618, E0732 and
  E0783 are reported as rustc does, and so are or-patterns without
  parentheses in `let` and parameters and rustc's other syntax rules
  for patterns and `let`-`else`. The parser accepts visibilities on
  variants and or-patterns as parameters, to report them as rustc does.
- Support constants in patterns (`N`, `i32::MAX`, `Self::N`, `<T as
  Tr>::N`), alone and as range ends, of integer types and `bool`. The
  exhaustiveness and range checks of bodies that name them wait for
  constant evaluation, which evaluates the constants first, as rustc
  does. E0029, E0080 ("runtime values cannot be referenced in
  patterns"), E0158, E0530, E0532 and E0533 are reported as rustc does.
- Support `#[repr(u8)]` and the other integer types on enums, which
  set the discriminant's type: its range, literal suffixes, casts and
  the tag's size, and allow discriminants on enums with fields.
  `#[repr(C)]` is accepted; malformed, conflicting and zero-variant
  `repr`s are reported as rustc does (E0084, E0539, E0565, E0566,
  E0600, E0732).
- Evaluate panics in constants: `Option::unwrap` is a `const fn`, as in
  rustc, and `None.unwrap()` in a constant is "evaluation panicked"
  (E0080). `core`'s other `Option` and `Result` methods are reported as
  conditionally const (E0658), with rustc's two errors.
- Support discriminants written as any constant expression (`A = 1 <<
  2`, `B = N + f()`, `C = Self::A as isize + 1`), typed by the enum's
  `repr`: each is an anonymous constant, evaluated with the others and
  as rustc does, only as far as each use needs, so casts, matches,
  constant patterns and code generation see the evaluated values.
  E0015, E0080, E0081, E0308, E0370 and E0391 are reported as rustc
  does, and so are `Self` and type parameters in a generic enum's
  discriminants.
- Report a negated literal out of range (`-129i8`) at the `-`, as rustc
  does.
- Check that associated types' traits hold where they are named:
  `<T as Tr>::Out` needs `T: Tr` (E0277) in signatures, fields,
  constants' types, bounds, `impl` headers and annotations in bodies,
  reported at rustc's positions, again where a body or a constant's value
  normalizes its signature, as rustc does. An unmet bound of a
  signature is reported once per item, for structs' bounds too.
- Report the mismatched types rustc reports after E0409, where a
  binding is by reference in one alternative and by value in another.
- Recover from malformed patterns and `else` branches with one error
  where rustc gives one: a trailing `|` in an or-pattern, `-` before a
  name, `let ... else` without a block or with `else if`, `else`
  without a block, a struct literal as a `match` scrutinee or `if`
  condition, and a missing `,` after an arm. `mut ref x` parses, and is
  reported as unstable.
- Fix a crash on `Self { .. }` of a struct whose type is in error.
- Fix type checking that took exponential time or overflowed the stack
  on associated types defined through each other: each normalization
  now normalizes a type once, stops at a cycle, and inference reports
  one as rustc's overflow (E0275). Elaborating bounds no longer
  compares each with all before it. rayon's `iter` module checks in
  milliseconds rather than never.
- Find the bounds of an associated type (`type Smaller: Dimension;`)
  when a goal, method call or normalization names it, as rustc does,
  rather than adding them to every function's bounds four levels deep:
  they hold however deeply associated types nest (methods of
  `<<T::Smaller as Dim>::Larger as Dim>::...` five levels down were not
  found), and ndarray's `lib.rs` checks in 25 ms rather than 24 s.
- Name the binding in "cannot borrow `s.a` as mutable, as `s` is not
  declared as mutable", as rustc does.
- Report duplicate items in `impl` blocks of traits (E0201), which
  produced invalid keel IR.
- Find inherent functions through qualified paths (`<S>::new()`).
- Update keel to `1aa8c53`, which fixes `-O` arm64 code for a sparse
  `switch` whose default target is the next block (`match o { Some(v)
  => v, None => -1 }` returned the payload for `None`). flux's own
  functions are local and use keel's private calling convention
  (`cc(fast)`) where it allows, one result at most; `extern` functions
  keep C's.
- Give macro calls a node kind per position: `MACRO_CALL` (items),
  `MACRO_EXPR`, `MACRO_TYPE` and `MACRO_PAT`.
- Support operators on references to integers and `bool` (`&a + b`,
  `a + &b`, `a += &b`, `-&a`, `!&a`, `&a == &b`, `&a < &b`), which
  match ergonomics makes common. `core` implements the operator traits
  for references as rustc's does, shifts for any integer type on the
  right, and `PartialEq` for `&A` and `&mut A`, so generic code takes
  references too; `<` on references is built in until `PartialOrd`.
  Mismatched operands are reported as rustc does (E0277 with E0308 for
  integers of different types, E0369, E0368, E0600), and operators that
  call trait methods in constants and `const fn`s are E0658 or E0015.
  `impl` blocks overlap (E0119) only if their whole headers unify, and
  E0119 names the trait with its arguments and `_` for what is left
  open (`From<W<_>>` for `W<_>`), as rustc does.
- Support `?` on `Option` and `Result`, returning `None` or the error
  converted with `core::convert::From` (new, in the prelude, with
  rustc's lossless integer conversions), with rustc's errors for the
  `Try` and `FromResidual` bounds (E0277), mismatched values (E0308)
  and `?` in constants and `const fn`s (E0572, E0658).

### Performance

- Run the corpus tests on every core, and print the files they spent
  longest on: `just corpus ~/.cargo/registry/src` takes 13 s instead of
  about two and a half minutes. Derives no longer look for a `Copy`
  derive among all of a crate's (windows' module tree lowers in 1.5 s
  instead of 2.0 s), and module scopes hash with FxHash. The grammar
  check looks up each pair of node and child kind once, rather than
  formatting both for every child.

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
