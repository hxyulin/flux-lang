# Changelog

## Unreleased

### Syntax

- Add checked `NonZeroU32` construction/access and preserve its zero niche.
  The self-compilation subset now runs unchanged parser events while
  retaining their eight-byte representation.

- Add a bounded embedded `std` facade over existing `core`/`alloc` APIs,
  with shared type identities and abort-only `thread::panicking`.
- Add `flux package build/run --manifest-path PATH [--bin NAME]` for
  standalone dependency-free edition-2024 binaries. Cargo supplies offline
  metadata; Flux/Keel compiles and links, with explicit unsupported-input
  rejection and no rustc fallback.

- Enforce `'static` local type annotations through MIR region constraints,
  including nested types, aliases, defaults, destructuring and later writes.
  Promote borrowed arrays of fieldless enum values through matches and
  unsizing while preserving destructor and discriminant-cycle checks.
  The self-compilation subset now executes the unchanged syntax-kind module.

- Accept a literal `$` at the end of a macro matcher group, including
  the compiler's own `T![$]` rule. Add a dedicated `just selfhost-subset`
  check that compiles real compiler modules and compares their execution
  with rustc; extend it to MIR interval sets.
- Filter ordinary and associated items with `#[cfg(...)]` before
  expansion and lowering, using the same validated predicate evaluator
  as `cfg!`. Add slice `partition_point`, used by compiler interval sets.

- Parse parenthesized range-to patterns as patterns rather than tuples,
  and preserve attributed tail expressions in blocks and macro expansions.
  Reenable their differential generator coverage. Compile the compiler's
  own choice tape and dataflow bitset in the runtime comparison harness;
  add unsigned integers' `is_multiple_of` method used by the bitset.

- Support reference results in `Fn`, `FnMut` and `FnOnce` bounds, with
  the result borrowing from the input (`F: Fn(&T) -> &U`), including
  equivalent single-lifetime `for<'a>` binders. Check each call's
  result lifetime and reject closures whose captured borrows cannot
  satisfy the bound, including returned opaque closures and recursive
  impl obligations. Preserve the input lifetime through type and
  associated-type substitution and trait-object lifetime defaults;
  validate binders before eliding them.

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
- Add `core::cmp::{Ordering, PartialOrd, Ord, max, min}` (`PartialOrd`
  and `Ord` in the prelude): `Ordering` with `is_lt` and the other
  tests, `reverse` and `then`; `PartialOrd`'s `partial_cmp` with `lt`,
  `le`, `gt` and `ge`; and `Ord`'s `cmp` with `max`, `min` and `clamp`,
  implemented for the integers, `bool`, `()`, references, `Option`,
  `Result` and `Ordering` as rustc's are. `<`, `<=`, `>` and `>=` on
  other types than the integers and `bool` call `PartialOrd`'s
  methods, references included, which replaces flux's built-in `<` on
  references; mismatched and unordered operands are reported as rustc
  does (E0308, E0369, E0277). The integers' `min` and `max` are now
  `Ord`'s, as in rustc. `#[derive(PartialOrd, Ord)]` on structs and
  enums compares fields in order and variants by discriminant, as
  rustc's does. Calls of trait methods in constants and `const fn`s
  are reported as rustc does: E0658 for `core`'s impls for its own
  types (`<i32 as Ord>::max`), E0015 for others; a method a reference's
  `impl` block has, under bounds that fail (`a.clamp(b, c)` where `a`
  is not `Ord`), is E0599's "exists but its trait bounds were not
  satisfied"; and a derived `PartialEq` on an enum without variants
  matches on `*self`, as rustc's does, rather than failing E0004.
- Support arrays and slices: `[T; N]` types, whose length is an integer
  or a constant (evaluated by flux's constant evaluator while types are
  checked, so its errors stop type checking, as in rustc), `[a, b]` and
  `[x; N]` (whose value must be `Copy` or a constant, E0277), indexing
  with `usize` as a value and a place (`a[i] = v`, `&mut a[i]`), with a
  run-time bounds check that aborts as rustc's panic does, `&[T]` and
  `&mut [T]` from references to arrays by rustc's unsizing coercion,
  and the built-in `len` and `is_empty`. Arrays are `Copy`, `Clone` and
  `PartialEq` when their elements are, built in until `core` has const
  generics. Type errors (E0308 with array sizes, E0277 for indices that
  are not `usize` and unsized values, E0608), borrow errors (indexing
  borrows the whole array: E0499, E0502, E0503, E0506; E0508 for moves
  out of elements; E0594 and E0596) and rustc's `unconditional_panic`
  lint and E0080 for indices known to be out of bounds are reported as
  rustc reports them. Code generation keeps values that contain arrays
  in memory, passed by address and returned through `sret`, references
  to slices as an address and a length, and drops arrays element by
  element.
- Support type aliases (`type Pair<T> = (T, T);`), generic or not, in
  modules and imports, in signatures, bodies and `impl` headers
  (`impl Ints { .. }` for `type Ints = Pair<i32>;`), and naming structs
  and enums in paths (`Ints::new()`, `Alias::Variant`), struct
  expressions and patterns, with the alias's generic arguments. Bounds
  on their parameters are not enforced, as in rustc. E0091, E0107,
  E0121, E0391 (a cycle of aliases), E0423, E0532 and E0603 are
  reported as rustc does.
- Support statics: `static X: T = v;` and `static mut X: T = v;`, whose
  initializers are evaluated at compile time like constants' (and may
  read other statics, a `static mut`'s initial value included), and
  which are places in memory: read, borrowed (`&X`) and passed by
  reference, and a `static mut` read and written in `unsafe` blocks.
  Code generation emits each as a keel data object, read-only unless
  `mut`. Errors are rustc's: E0133 for a `static mut` used outside
  `unsafe`, edition 2024's deny-by-default `static_mut_refs` for
  references to one (`&M`, `&mut M`, method calls that borrow it),
  E0594, E0596 and E0507 for writing, mutably borrowing or moving out
  of a static, E0277 for a static whose type holds a raw pointer, E0530
  for bindings that shadow one, E0080 for a constant reading a `static
  mut`, and E0015 for non-`const` calls in initializers. Statics whose
  types hold references are not supported yet.
- Support functions as values: a path to a function (`double`,
  `S::new`, `<S as Tr>::m`, `id::<u8>`) is a value of its function item
  type (`fn(i32) -> i32 {double}`), which has no size and coerces to a
  function pointer type (`fn(i32) -> i32`, `unsafe fn()`) where one is
  wanted: in `let`s, arguments, fields, return values, `as` casts, and
  `if` and `match` arms and array elements naming different functions. Values of either
  type may be called, and pointers stored and passed anywhere. Calls of
  function items are direct; calls through pointers are keel's indirect
  calls, and a function whose address is taken uses C's calling
  convention rather than `cc(fast)`, which keel allows for direct calls
  only. `core` implements `Clone` for function pointers of up to three
  parameters. Errors are rustc's: E0308 for mismatched signatures and
  unsafe functions where safe pointers are wanted, E0133 for calling an
  `unsafe` one outside `unsafe`, E0618, E0061, and calls through
  pointers in constants and `const fn`s. The message for calling an
  `unsafe` function outside `unsafe` is now rustc's. Constructors of
  tuple structs and variants as values, `extern` function pointer types
  and casts of functions to integers are not supported yet.
- Support items inside function bodies and constants' values:
  functions, structs, enums, `impl` blocks, traits, constants, statics,
  type aliases, `use` declarations and inline modules declared in a
  block. As in rustc, a block with items is an anonymous module: its
  names are visible throughout it, before their declaration too, and
  in the blocks inside it, shadowing outer ones; `self` and `super`
  name the module around it, and its private items are private to that
  module. An inner function cannot use its outer function's locals
  (E0434), generic parameters or `Self` (E0401), nor a constant a local
  (E0435); a file module may not be declared in a block.
- Support closures (`|x| x + 1`, `move |x: i32| -> i32 { .. }`), with
  the `Fn`, `FnMut` and `FnOnce` traits in `core` (and the prelude),
  named in bounds as `Fn(A, B) -> C`. A closure's signature comes from
  its annotations, from the `Fn`-family bounds of what it is passed to
  (which also fix its kind), or from its calls. What it captures is
  found as rustc's edition-2021 analysis finds it: precise places
  (`s.a`, `*r`), by shared or mutable reference or by value, all by
  value with `move`, with its kind from what its body does. Closures
  are called directly, from fields (`(self.f)(x)`), and through
  generic parameters and references, and are `Copy` when what they
  capture is. Each closure's body is MIR of its own, checked like a
  function's, with shims for the traits beyond its kind, and is
  monomorphized with its owner. The borrow checker follows the loans
  closures hold through the values they are stored in, the calls they
  are passed to and `&mut` arguments, with rustc's messages (E0499,
  E0502, E0503, E0506, E0597, E0382, E0507, E0594, E0596, E0525,
  E0593, E0631, E0618, E0057, E0267). Function items and pointers
  implement the `Fn`-family traits (`opt.and_then(half)`,
  `iter.map(double)`), and closures that capture nothing coerce to
  function pointers. Calling `Fn::call` by name and implementing the
  traits are rejected as unstable, as in rustc. Closures returning
  references, `async` closures and closures in constants are not
  supported yet.
- Support ranges (`a..b`, `a..=b`, `a..`, `..b`, `..=b`, `..`) as
  `core::ops`'s `Range` structs, with `contains`, and `core::iter`:
  `Iterator` with `map`, `filter`, `filter_map`, `enumerate`, `zip`,
  `chain`, `take`, `skip`, `take_while`, `skip_while`, `step_by`, `rev`
  and consumers (`count`, `last`, `nth`, `sum`, `product`, `fold`,
  `for_each`, `any`, `all`, `find`, `find_map`, `position`, `max`,
  `min`, `max_by_key`, `min_by_key`, `eq`), `DoubleEndedIterator`,
  `IntoIterator` with the blanket impl for iterators, `Sum` and
  `Product`, and the integer ranges as iterators. `for` loops lower
  as rustc's do, with labels on `for`, `while` and `loop` for `break`
  and `continue`, and rustc's errors for what is not an iterator,
  refutable patterns (E0005), `break` with a value (E0571) and
  undeclared labels (E0426).
- Add the `Option` and `Result` methods that take closures: `map`,
  `map_or`, `map_or_else`, `and_then`, `or_else`, `unwrap_or_else`,
  `filter`, `ok_or_else`, `is_some_and`, `is_none_or`, `inspect` and
  `zip`; `map_err`, `is_ok_and` and `is_err_and`.
- An `impl` block's parameter that only its bounds' associated types
  decide (`B` in `impl<B, I, F: FnMut(I::Item) -> B> Iterator for
  Map<I, F>`) is constrained, as in rustc, and found from those types
  where the block is used. `T::Item` in `impl<T: It> Tr for T` names
  `T`'s bound's `Item` rather than the block's own, and a bound in
  scope on a type parameter keeps blanket `impl` blocks from deciding
  its associated types.
- Support lifetime syntax: lifetime parameters on functions, structs,
  enums and `impl` blocks with their bounds (`<'a, 'b: 'a, T: 'a>`,
  `where 'a: 'b, T: 'a`), `&'a T`, `&'a self`, `'static`, `'_` and
  lifetime arguments (`Parser<'a>`). Signatures record each type's
  lifetimes beside it (`flux_ty::regions`), with rustc's elision rules
  for function signatures, `impl` headers, constants and statics, the
  variances of structs' parameters, and the outlives requirements
  their fields and functions' parameters imply. rustc's errors for
  undeclared and outer lifetimes (E0261, E0401), elision that finds
  nothing (E0106, with its help), `'static` and `'_` as parameters
  (E0262, E0637), duplicate and shadowing names (E0403, E0496), wrong
  numbers of lifetime arguments (E0107, E0747) and unused lifetime
  parameters (E0392) are reported.
- The borrow checker infers regions as rustc's NLL does: a region
  variable for every lifetime in every local's type, each borrow and
  each callee's lifetimes, outlives constraints from subtyping (with
  variance), reborrows and callees' bounds, values from liveness over
  the body's points, and the function's lifetime parameters as
  universal regions that must outlive only what its bounds say
  ("lifetime may not live long enough"). A loan is in scope where its
  region reaches from where it is made, until what it borrows is
  overwritten, which replaces the locals-holding-loans approximation,
  and is checked at the function's end too: returning a borrow of a
  local, parameter or temporary is E0515. Constant expressions borrowed
  (`&5`, `&[1, 2]`) are promoted to static memory, as in rustc.
- Support references anywhere: returned (`fn first(&self) -> &T`,
  `Option<&T>`), in structs and enums (`struct Parser<'a> { src: &'a
  [u8] }`), tuples and other references, in `mut` bindings, bindings
  declared without a value and `break` values, and returned by
  closures. A closure's body is checked first, and tells its creator
  how the lifetimes of what it captures, its parameters and its result
  must relate, where it is made and where it is called; one passed to
  a generic function may not keep its parameters' references (not
  supported yet). Trait `impl` blocks' methods may not ask more of
  their lifetimes than their traits' ("`impl` item signature doesn't
  match `trait` item signature"), and their headers must hold for
  every lifetime (others are not supported yet). Borrowed temporaries
  die at the end of their scope, so one still borrowed there is E0716.
- Add `core::slice`: slices' `first`, `last`, `get` and `iter`, with
  the iterator `Iter<'a, T>` (from both ends), and `IntoIterator` for
  `&[T]`, so `for x in slice` and `for x in &array` work (the latter as
  a slice of the array). Of several `impl` blocks whose headers match,
  the one whose bounds hold is chosen, as rustc's winnowing does.
- Support shared references in constants and statics (`const P: &[i32]
  = &[2, 3];`, `static T: &[Entry] = &[...]`): constant evaluation
  gives a reference to a copy of what it borrows, and code generation
  lays it out as a data object the reference points to, with the
  linker filling in addresses inside statics. A constant's value is
  `'static`, so borrowing its locals is E0597, while temporaries in it
  live forever, as in rustc. Mutable ones are not supported yet.
- Accept trait `impl` blocks for references whose parameters have
  bounds (`impl<'a, T: Copy> IntoIterator for &'a Bag<T>`): of several
  `impl` blocks whose headers match, one whose unmet bounds are all on
  generic parameters is chosen over one whose bounds fail for known
  types. In a trait `impl` block's methods, `Self::Out` and `<Self as
  Tr>::Out` are the block's own type, with its lifetimes, so `fn
  into_iter(self) -> Self::IntoIter` matches its trait and returns a
  value borrowing `self`. References to references coerce to the
  reference expected (`&&T` to `&T`, rustc's deref coercion), and
  references to sized values may be passed to C-variadic functions.
- Add a scaling test of many regions and constraints in a long body,
  and an ignored one recording a quadratic case rustc's NLL shares: a
  `mut` reference reassigned from each new borrow through a body.
- Fix closures that use what a captured reference points to. A
  non-`move` closure calling a method through a captured reference
  (`let r = &s; || r.get()`, `|| slice.len()`) captures `*r`, and its
  body looked for `r` instead: it began with `unreachable` and trapped.
  Reborrowing a captured `&mut` for a call (`|| f(m)`) and comparing
  captured `&mut` references overflowed the compiler's stack, and
  patterns matching through a captured reference (`|| match r { S { v }
  => *v }`) and `let _ = r` trapped. The body now finds such places in
  what the closure captures, with the dereferences autoderef,
  reborrows and patterns add. `len` and `is_empty` borrow their array
  or slice and comparisons of `&mut` references reborrow them, so
  closures no longer move them (E0382 where rustc reports E0505).
  Code MIR building never expects to reach panics in debug builds.
- Support byte literals (`b'a'`, a `u8`) and byte strings (`b"..."`,
  `br#"..."#`, a `&'static [u8; N]` that coerces to `&[u8]`), in
  patterns too for bytes (`b'a'..=b'z'`), and in constants and
  statics. Literals' escapes are checked as rustc's lexer checks them,
  with its messages ("unknown byte escape: `q`", "out of range hex
  escape", "non-ASCII character in byte string literal", ...); `'ab'`
  is one char literal, reported as rustc does. `core` gains the ASCII
  methods of `u8` (`is_ascii_alphabetic`, `is_ascii_digit`,
  `to_ascii_lowercase`, `eq_ignore_ascii_case`, `make_ascii_uppercase`,
  ...) and of `[u8]` (`is_ascii`, `eq_ignore_ascii_case`,
  `make_ascii_lowercase`, `make_ascii_uppercase`). A constant reference
  to an array passed where a slice is wanted (`f(b"ab")`, `f(ARRAY)`)
  is unsized, which it was not.
- Support `char`: literals (`'a'`, `'\u{1F600}'`), comparisons, `as`
  casts to the integers and from `u8` (others are rustc's E0604, and
  `char as bool` E0054), constants, and patterns, ranges included,
  whose exhaustiveness is checked as rustc's is, without the
  surrogates, with its witnesses (`'\0'..='`'`). `core` implements
  `Copy`, `Clone`, `PartialEq`, `Eq`, `PartialOrd`, `Ord` and the
  `From` conversions for it, and gains `core::char` with `char::MAX`,
  `REPLACEMENT_CHARACTER`, `from_digit`, `to_digit`, `is_digit`,
  `len_utf8`, `len_utf16`, `is_whitespace` and the ASCII methods.
- Support raw pointers made from references: a `&T` where a `*const
  T` is wanted, and a `&mut T` where a `*mut T` or `*const T` is, is
  coerced (a `&mut` reborrowed), and `as` casts references to
  pointers (`r as *const T`, `&array as *const T`), pointers to
  pointers (a pointer to a slice keeps its length, or drops it as a
  thin pointer), pointers to integers and integers to pointers, with
  rustc's errors for the others (E0606, E0607, E0054). `core` gains
  `as_ptr` and `as_mut_ptr` on slices, `char::from_u32` (and
  `from_u32_unchecked`, and `core::char`'s functions), ranges of
  `char`s that iterate (skipping the surrogates), and `core::ffi`,
  whose `CStr` names the type of `c"..."` and whose `c_int`, `c_char`
  and the others name C's integer types, so a program can print a
  byte string with `printf(c"%.*s".as_ptr(), s.len() as i32,
  s.as_ptr())`. `char` may be passed to C-variadic functions.
- Support `str` and string literals (`"..."`, `r#"..."#`, a `&'static
  str`): `str` is unsized like `[u8]`, so a `&str` is a pointer and a
  length, in constants and statics too. `core::str` gives it `len`,
  `is_empty`, `as_bytes`, `as_ptr`, `bytes`, `chars` and
  `char_indices` (UTF-8 decoding, from both ends), `is_char_boundary`,
  `is_ascii`, `eq_ignore_ascii_case`, `starts_with`, `ends_with`,
  `contains` and `find` with a `&str` or `char` pattern, `==`, `<` and
  the other comparisons (`PartialEq`, `Eq`, `PartialOrd`, `Ord`), and
  `str::from_utf8` (also `core::str::from_utf8`), which validates as
  rustc's does and reports a `Utf8Error` with `valid_up_to` and
  `error_len`. Indexing a `str` by an integer is rustc's E0277; by
  ranges, and the methods that need parts of a string, are not
  supported yet. String literals in patterns (`match s { "add" => ..
  }`) compare with `<str as PartialEq>::eq`, as in rustc, and, like
  byte strings, match a reference without seeing through it; for
  exhaustiveness they cover nothing (`&_` not covered).
- Find slices' methods on arrays (`arr.iter()`, `arr.first()`,
  `(&mut arr).last()`): past the receiver's last dereference, an array
  is borrowed and unsized to a slice, as rustc's method probe does.
- Iterate arrays by value (`for x in [1, 2, 3]`, `array.into_iter()`)
  and by mutable reference (`for x in &mut array`, `iter_mut`):
  `IntoIterator` for `[T; N]`, `&[T; N]` and `&mut [T; N]` is built into
  the compiler, as arrays' `Clone` is, with `core::array::IntoIter`,
  which drops the elements it has not given. `core` gains
  `slice::IterMut`, `mem::ManuallyDrop`, `marker::PhantomData` and
  intrinsics (`#[rustc_intrinsic]`) that monomorphization replaces with
  MIR: reading and writing through references, unsizing an array of a
  generic type, and subslicing (`Rvalue::Subslice`).
- Support ranges as indices (`&a[1..3]`, `&mut s[lo..hi]`, `a[..=n]`,
  `&a[..]`) and `core::ops::{Index, IndexMut}`, which the program may
  implement (`grid[(r, c)] = 1`). Indexing other than an array's or
  slice's by a `usize` calls `Index::index` or, where the place is
  assigned to or borrowed mutably, `IndexMut::index_mut`, found on each
  type the base dereferences to and then on the slice an array unsizes
  to, as rustc's `lookup_indexing` does. `core`'s impls for `[T]` panic
  where rustc's do. Diagnostics follow rustc's (E0277 naming
  `std::ops::Range`, E0594 and E0596 without `IndexMut`, E0658 in
  constants, E0502 for `&mut s[..s.len() - 1]`, as rustc borrows the
  base first). Reading a slice's length (`len`) now keeps the reference
  it reads live for the borrow checker, which missed conflicts with it.
- Add slices' methods to `core`: `swap`, `reverse`, `rotate_left`,
  `rotate_right`, `split_at`, `split_at_mut`, `copy_from_slice`,
  `clone_from_slice`, `fill`, `contains`, `starts_with`, `ends_with`,
  `windows`, `chunks`, `chunks_exact`, `chunks_mut`, `first_mut`,
  `last_mut`, `get_mut`, `split_first`, `split_last`, `binary_search`
  (`_by`, `_by_key`, as rustc's finds among equal elements), `is_sorted`,
  and `sort`, `sort_by`, `sort_by_key` and their unstable forms, a
  stable in-place merge sort; `PartialEq`, `Eq`, `PartialOrd` and `Ord`
  for slices, which may be compared as places (`a[..2] == b[..2]`);
  `Iterator::size_hint` and `rposition`, and `ExactSizeIterator`.
  Arrays are `Eq` where their elements are.
- Compare arrays with slices and references to them, either way round
  (`s == [1, 2]`, `[1, 2] == *s`, `w == b"fn"`): built in, as arrays'
  `PartialEq` is, comparing them as slices.
- Index strings by ranges of bytes (`&s[1..4]`, panicking unless they
  are within the string at character boundaries), and add `str`'s
  `split_at`, `trim`, `trim_start`, `trim_end`, `split`, `lines` and
  `split_whitespace`, and `char::encode_utf8`.
- Fix two-phase borrows: once its call has activated it, a method
  receiver's `&mut` conflicts with later shared borrows (E0502), which
  flux missed; as in rustc, an array unsized for a method's receiver is
  not borrowed two-phase (`a.swap(0, a.len() - 1)` is E0502).
- Fix coercions inside tuples, which were miscompiled: a tuple whose
  element was unsized (`&[i32; 5]` to `&[i32]`) was built with its
  uncoerced type, then copied as a tuple of the coerced type, so an
  array of such tuples (`[(10, &a)]`) or one passed to `Some` held a
  slice with a garbage length, and one nested in a tuple crashed keel.
  As in rustc, where the context wants a type, each tuple element, `if`
  and `match` branch, `break` value, method argument and operand of `&`
  (`&[x, y]` where a `&[T]` is wanted) is a coercion site for its part
  of it, and the tuple, branch or loop has the type wanted. Branches
  without one meet at rustc's least upper bound (`if c { &a } else {
  slice }`, an arm coerced to the arms' type or the arms before to its
  own; function items and closures to a function pointer). Mismatches
  in branches and `break` values are reported where rustc reports them.
  An `as` cast to a reference type is a coercion (`b"ab" as &[u8]`),
  and one that is not is rustc's non-primitive cast.
  A `&mut T` reborrowed as a `&T` (or deref-coerced) into a temporary
  gets a temporary of the coerced type.
- Validate MIR: every assignment's value has its place's type, every
  call's arguments and result its callee's parameter and return types.
  `flux_mir::validate` runs over every body built and every instance
  compiled in debug builds, and over every corpus file that builds,
  with `just corpus`. It found that function items' values were `()`
  (now `Constant::FnItem`), `CStr::as_ptr` stored a `&CStr` in a
  `*const i8` (now a cast), and the end of a block that diverged stored
  `()` in a place of type `!` or of the function's return type.
- Fix code generation of a call that never returns into a place kept in
  memory (`None => panic(...)` in `Option<[T; N]>::unwrap`), which
  crashed the compiler, and constant evaluation of a static indexing
  another static at a computed index, which panicked.
- Add the integer methods rustc's `core` has beyond the basic ones:
  `checked_`, `wrapping_`, `saturating_` and `overflowing_` arithmetic
  (`add`, `sub`, `mul`, `div`, `rem`, `neg`, `shl`, `shr`, `pow`,
  `abs`, the Euclidean divisions and the mixed-sign ones), `div_euclid`,
  `rem_euclid`, `ilog2`, `ilog10`, `ilog`, `isqrt`, `midpoint`,
  `next_power_of_two`, `div_ceil`, `next_multiple_of`, `leading_ones`,
  `trailing_ones`, `swap_bytes`, `reverse_bits`, `unbounded_shl`,
  `cast_signed` and the byte-order conversions. All are `const`; in
  constants, `next_power_of_two` and `next_multiple_of` report overflow
  and the logarithms and square roots their panics, as rustc does.
- Add to `core::iter`: `size_hint` (rustc's for every adapter and
  range), `ExactSizeIterator` (in the prelude), `copied`, `cloned`,
  `flat_map`, `flatten`, `peekable`, `scan`, `map_while`, `inspect`,
  `cycle`, `fuse`, `by_ref`, `reduce` and `rposition`, the back ends of
  more adapters, `Iterator` for `&mut I`, and `Sum` and `Product` of
  references to integers, with rustc's messages when they are misused
  (E0271's "expected ... to be an iterator that yields", `Sum`'s and
  `Product`'s). `Option` gains `copied`, `cloned`, `as_ref`, `as_mut`,
  `take`, `take_if`, `replace`, `insert`, `get_or_insert`,
  `get_or_insert_with`, `iter` and `IntoIterator`; `core::mem` gains
  `swap`, made of `core`'s `read` and `write` intrinsics, and `replace`;
  slices' iterator is `Clone`. `ExactSizeIterator::len` checks the
  bounds of `size_hint` agree, as rustc's does. `core` may name lifetimes in its bounds'
  associated types (`I: Iterator<Item = &'a T>`) where they never reach
  what it returns.
- Fix associated types of type parameters that a blanket `impl` block
  decided without its bounds holding: `<U as IntoIterator>::Item` for
  `U: IntoIterator` became `<U as Iterator>::Item` (in signatures and
  fields), which miscompiled. A struct's field types for known
  arguments now find the parameters only an `impl` block's bounds
  decide (`T` of `Copied<I>`'s `Iterator` block) and closures' return
  types, prefer specific blocks to blanket ones, and leave associated
  types of parameters and unknown types to inference. Obligations are
  selected before closure arguments are checked, as rustc does, so
  `it.copied().map(|p| p.x)` knows `p`'s type, and bounds on a method's
  own parameters that no argument mentions are reported at its name.
- Add `core::default::Default` (in the prelude) for the integers,
  `bool`, `()`, `Option`, tuples of up to twelve elements, `Range` and
  `RangeFull`, and arrays of up to 32 elements, built into the compiler
  as their `Clone` is; `#[derive(Default)]` on structs, and on enums
  with a `#[default]` unit variant, with rustc's errors (E0665,
  "multiple declared defaults", non-unit and valued `#[default]`, and
  `#[default]` without the derive); `Option::unwrap_or_default` and
  `get_or_insert_default`, `Result::unwrap_or_default` and
  `core::mem::take`. Fix functions returning values without size (`[T;
  0]`), which produced invalid keel IR.
- Add `core::fmt` with the `Debug` trait, an opaque `Formatter`,
  `Result` and `Error`, implemented for the primitive types, references,
  slices, arrays (built in, as the slice's), tuples, `Option`, `Result`,
  `Ordering` and the ranges, and `#[derive(Debug)]` on structs and
  enums, whose unmet bounds read "`T` doesn't implement `Debug`" at the
  field, as in rustc. Nothing formats yet. Fix `#[derive(PartialOrd)]`
  inside `core` outside its root module, which named `Option` without
  its arguments.
- Fix derives of `Clone` and `Copy` together on a struct with fields
  inside `core`, whose `clone` named `::core::clone`, which `core`
  cannot, and failed in MIR building of unrelated programs ("call of a
  value that is not callable"); `Utf8Error` derives its traits again.
  `Debug` and `Default` for `char` and `str` (`&str`), `Result`'s
  `unwrap`, `expect`, `unwrap_err` and `expect_err`, `Option::expect`,
  and `size_hint` for ranges of `char`.
- Support comparing tuples: `core` implements `PartialEq`, `Eq`,
  `PartialOrd` and `Ord` for tuples of up to twelve elements as rustc's
  does (`ne` with each pair's `!=`, `lt` and the others through each
  pair's `partial_cmp` and the last pair's operator), so `==`, `<` and
  the others work on tuples, by reference too, and derives of structs
  with tuple fields; other operators on tuples are E0369 as in rustc.
  Arrays are `PartialOrd` and `Ord` where their elements are, built
  into the compiler as calls of the slices' impls, as rustc's `core`
  has them.
- Fix normalizing an associated type through an `impl` block whose
  bounds do not hold: field types picked any block whose header
  matched, so `FlatMap`'s iterator of an array was the array itself
  (`Option<[i64; 2]>` rather than `Option<IntoIter<i64, 2>>`), and
  signatures fell back to such a block. A block is now chosen only if
  it is the one whose header matches and whose bounds can hold, and
  for a type parameter only if they surely hold, since a bound in scope
  on it decides otherwise; else the associated type stays as it is,
  as in rustc. Field types know arrays' built-in `IntoIterator` too.
  `tests/run/into_iterator_args.rs` passes arrays, slices, `Option`,
  ranges and iterators to `U: IntoIterator` parameters.
- Record what MIR building lowers to `unreachable` because type checking
  should have ruled it out (`Body::bugs`), instead of asserting on it.
  Where the program has errors (a body naming what a failed import
  should have brought) that is expected and no longer panics debug
  builds; in a program without errors `build_file` panics in debug
  builds and reports an internal compiler error in release builds, so
  such code is never compiled to a trap, and `just corpus` fails on it.
  A constant naming a failed import's type no longer reports "entering
  unreachable code" when an array length needs it, and a call of an
  associated function of such a type no longer panics.
- Support trait `impl` blocks for some lifetimes only: a header naming
  a lifetime twice (`impl<'a> PartialEq for Token<'a>`, whose `Rhs` is
  `Token<'a>` too; `impl<'a> Add for &'a V`), naming `'static` (`impl
  Tr for &'static S`), or bounding its lifetimes (`impl<'a, 'b: 'a>`).
  A block is still chosen by types alone; then the borrow checker
  proves each call's trait bounds by it, as rustc's does, so its
  header's lifetimes, its bounds and those of the blocks its own bounds
  choose constrain the call's. Derives on types with lifetime
  parameters work (`#[derive(Clone, Copy, PartialEq, Debug)] enum
  Token<'a>`), and misuses are rustc's errors: E0597, "lifetime may not
  live long enough" (now once per lifetime that fails, so both of an
  invariant type's), and E0521, "borrowed data escapes outside of
  function", for a parameter's lifetime a call needs to be `'static`.
  A copy of a value whose type is `Copy` for some lifetimes only
  (`impl Copy for X<'static>`) needs them, as in rustc. `Drop` impls for some lifetimes only are E0366, and with lifetime
  bounds the struct does not have or imply E0367; a lifetime only a
  trait `impl` block's associated type names is E0207.
- Bounds of functions and `impl` blocks may name lifetimes in what they
  say associated types are (`I: Iterator<Item = &'a T>`), outside
  `core` too: the body sees the bound's lifetimes, and callers must
  meet them (E0597, E0515, E0521, as in rustc). Lifetimes in a bound's
  trait arguments (`T: Tr<&'a U>`) are still `core`'s only.
- `core::ffi::CStr` is `PartialEq` (with `CStr` and `&CStr`), `Eq`,
  `PartialOrd`, `Ord` and `Debug`, comparing bytes as rustc's does, and
  has `count_bytes`, `is_empty`, `to_bytes`, `to_bytes_with_nul`,
  `to_str`, `from_bytes_with_nul` and `from_bytes_until_nul`, with
  rustc's `FromBytesWithNulError` and `FromBytesUntilNulError`; `&CStr`
  and `&[T]` are `Default`. `*a == *b` compares `str`s and `CStr`s.
- Fix overwriting a reference while what it pointed to is still
  borrowed (`out[n] = Token::Ident(&src[a..b])` in a loop), which was
  E0506: as in rustc, a write of a pointer leaves loans of what it
  points to alone. `&[]` is promoted for any element type, as in rustc.
- Support `impl Trait`. In a function's parameters (`f: impl Fn(i32)
  -> i32`, `it: impl Iterator<Item = u32>`, nested ones too) each is an
  anonymous type parameter after the others, named as written, which
  turbofish cannot give (E0107). In a function's or inherent method's
  return type (`-> impl Iterator<Item = u64>`, `-> impl Fn(i32) -> i32
  + '_`, `Option<impl Fn()>`, `use<'a>`) each is an opaque type
  (`TyKind::Opaque`): callers know it by its bounds (methods, calls,
  `for` loops, `Copy` only if bounded), its body decides its hidden
  type, which must meet them (E0277, E0271) and be the same for every
  return (E0308, "expected `u32`, found opaque type"), and code
  generation reveals it, so returned closures and iterator pipelines
  compile. As in edition 2024, it captures every generic parameter and
  lifetime in scope unless `use<..>` says otherwise, keeping what they
  borrow borrowed where it is used, and the borrow checker relates its
  hidden type to them as rustc's member constraints do (E0700, E0597,
  E0373 for a returned closure that borrows, "lifetime may not live
  long enough" for `Item = &'a T` it does not meet). `impl Trait`
  elsewhere is rustc's E0562 (or E0658 in type aliases and associated
  types); in trait methods' return types, nested in an opaque type's
  bounds, and in bounds' generic arguments it is not supported yet.
- Support array lengths and repeat counts written as any constant
  expression (`[bool; LIMIT + 1]`, `[u8; double(N)]`, `[T; { 3 }]`,
  `[u8; Self::CAP * 2]`): each is an anonymous constant of type `usize`,
  as in rustc, evaluated while types are checked, with its errors
  (E0080) at the expression and rustc's for locals (E0435, in types
  too), generic parameters ("generic parameters may not be used in
  const operations", "constant expression depends on a generic
  parameter") and a generic `Self`. A constant needing one that waits
  for its own length is a cycle, no longer a panic.
- Support const generics (`min_const_generics`): const parameters of
  integer types, `bool` and `char` on structs, `impl` blocks, functions
  and traits (`struct Stack<T, const N: usize>`, `impl<T, const N:
  usize> Tr for [T; N]`, `trait Pick<const I: usize>`), used as values,
  array lengths, repeat counts and arguments; arguments as literals,
  constants, parameters, `{ expr }` blocks (anonymous constants of the
  parameter's type) and `_`, or inferred (`sum([1, 2, 3])`).
  Constants are types in the type system (`TyKind::Value`, arrays'
  lengths among them), so substitution, inference, `impl` matching and
  monomorphization handle them as types; MIR's `Constant::TyConst` is a
  parameter's value until substituted. rustc's errors: E0770, forbidden
  parameter types, E0747, E0107, E0308 with sizes and values, E0284,
  E0158, E0530, and "generic parameters may not be used in const
  operations".
- Move arrays' traits from the compiler into `core::array`, as
  `impl<T, const N: usize>` blocks as rustc's `core` writes them:
  `Copy`, `Clone` (a copy for `Copy` elements, as rustc's
  specialization makes it, through the new `is_copy` intrinsic),
  `PartialEq` with arrays, slices and references to slices either way
  round, `Eq`, `PartialOrd`, `Ord`, `Debug`, `Default` for up to 32
  elements, and `IntoIterator` for `[T; N]`, `&[T; N]` and `&mut [T;
  N]`, with `array::IntoIter<T, N>` now rustc's type. The compiler's
  shims and special cases for them are gone. Add `array::from_fn`
  (through the new `uninit` intrinsic) and arrays' `map`, `as_slice`,
  `as_mut_slice` and `each_ref`. Comparing arrays of different element
  types (`[1u8] == [1i32]`) is E0277, no longer E0308.
- Expand macro calls. `macro_rules!` macros match as rustc's macro
  parser does (every fragment specifier, nested repetitions with
  separators, rustc's errors for no rule matching, ambiguity and
  fragments followed by what their kind does not allow), with textual
  scope (`#[macro_use]` modules, macros in function bodies),
  `#[macro_export]`, `$crate`, mixed-site hygiene for locals and labels,
  and rustc's recursion limit of 128. Calls work in item, statement,
  expression, pattern and type position. `core` defines rustc's
  `assert!`, `assert_eq!`, `assert_ne!`, the `debug_assert!` family
  (on, as in rustc without `-O`), `panic!`, `unreachable!`, `todo!`,
  `unimplemented!`, `matches!`, `format_args!`, `write!`, `concat!`,
  `stringify!`, `line!`, `column!`, `file!`, `module_path!`, `cfg!` and
  `compile_error!`, with `core::fmt::Display` and the other formatting
  traits that `{}`, `{:?}`, `{:x}` and friends check their arguments
  against, where rustc reports them; messages are not formatted (a panic
  aborts without one) but constant evaluation reports literal ones as
  rustc does. Each expansion is parsed into a tree of its own whose
  ranges are where its tokens came from, so diagnostics point where
  rustc's do: into the definition of a local macro, at the call of
  `core`'s.
- Format and print. `core::fmt` formats as rustc's does, byte for
  byte: `Formatter` with fill, alignment, sign, `#`, `0`, width and
  precision (`pad`, `pad_integral`, the options' accessors), `Write`
  with `write_str`, `write_char` and `write_fmt`, `Arguments` built by
  `format_args!` as rustc lowers it (positional, named and captured
  arguments, `{:1$}`, `{:.*}`, `{:width$}`), `Display`, `Debug`,
  `LowerHex`, `UpperHex`, `Octal`, `Binary`, `LowerExp` and `UpperExp`
  for the integers, `bool`, `char` and `str` as rustc's have them,
  `Debug` for tuples, slices, arrays, references, `()`, `Option`,
  `Result`, `Ordering`, the ranges, `CStr` and `Utf8Error`, the
  `debug_struct`, `debug_tuple`, `debug_list`, `debug_set` and
  `debug_map` builders with `{:#?}`'s indentation, and
  `#[derive(Debug)]` as rustc expands it. `print!`, `println!`,
  `eprint!`, `eprintln!` and `dbg!` are there as `std`'s prelude has
  them, writing through C's `write`, stdout buffered by lines as std's
  is; `write!` and `writeln!` work on any `fmt::Write`. Format string
  errors point inside the string, where rustc's do.
- Panics print rustc's message to stderr, `thread 'main' (id)
  panicked at file:line:column:` and the message, then abort: `panic!`
  and `unreachable!` with their formatted messages, failed assertions
  (`assert_eq!`'s `left`/`right`), `unwrap` and `expect` of `Option`
  and `Result` (with the error's `Debug`), indices past the end, slice
  and string ranges out of bounds or inside a character, division by
  zero and `clamp`. `#[track_caller]` works as in rustc: such a
  function reports where it was called, through others, in
  `core::panic::Location::caller()` and its panics. `tests/run` now
  compares stderr too, and constant evaluation reports `panic!("{}",
  s)` of a `&str` with its message.
- Fix bugs the second round of example programs found. Type aliases of
  arrays whose lengths name constants (`type Pair = [u32; N]`), array
  lengths written as expressions in items a macro defines, `core`
  functions reached only as values (`.all(u8::is_ascii_hexdigit)`,
  `.filter(Result::is_ok)`, pointers to them) and closures that only
  call a captured function (`|b| pred(b)`) compile instead of crashing
  code generation. A local whose type is an error reaching MIR, or an
  instance with arguments that are not concrete, is now an internal
  compiler error rather than invalid code. Arguments other than closures
  are coerced to their parameters before closures are checked, as in
  rustc, so `fold(0i32, |acc, x| acc.wrapping_add(x))` knows `acc`.
- Relate a closure passed to a generic function through the callee's
  `Fn`-family bounds, as rustc relates closure signatures: closures may
  return and keep references to their parameters (`map(|i| &i.qty)`,
  `filter_map(|s| match s { .. })`, `map(|(slot, f)| *slot = f)`), a
  call's result holds what its closure captures only as its signature
  says (`map_err(|code| Error::Bad { key, code })`), adapters' items
  have their closures' lifetimes (`impl Iterator<Item = &'static str>`
  from `map(|i| i.name)`), and a closure's annotated parameters take the
  bound's lifetimes (`map(|s: &str| s.trim())`). A bound's own
  references (`&T` in `F: Fn(&T)`) are higher-ranked: keeping or
  returning them is E0521 or "lifetime may not live long enough", as in
  rustc. A `for` loop over an opaque iterator no longer keeps what it
  borrows in its items.
- Match rustc's diagnostics more closely: names an unsupported pattern
  binds stay bound, errors involving error types or receivers of unknown
  type after other errors are not reported, nor are struct patterns
  naming a failed import, an associated type's unresolved bound stops
  type checking, a `match` reports only its first arm that does not
  fit, identical errors from several expansions of a macro are reported
  once, conflicts with closures' unique captures are E0500, E0501 and
  E0524, and a name followed by parentheses or braces where an item
  should start is rustc's "missing `fn` or `struct` for function or
  struct definition" (and its other forms).
- Add `core::ops::ControlFlow` and rustc's `Try` and `FromResidual`
  traits (unstable to name, as in rustc) for `Option`, `Result` and
  `ControlFlow`, with `Iterator::try_fold`, `try_for_each` and
  `DoubleEndedIterator::try_rfold` bounded by them, and
  `core::convert::Infallible`. `?` works on a `ControlFlow`, with
  rustc's errors where the function returns another type. Structs and
  enums may give their type parameters defaults, which types that leave
  them out get (`ControlFlow<i32>`, `W<'a>`), with rustc's E0107 for
  too many or too few arguments; an `impl` block's supertraits are
  checked with its own associated types normalized.
- Add `bool::then_some` and `bool::then`, `core::cmp::Reverse`,
  `core::iter::{empty, once, once_with, repeat, repeat_with, repeat_n,
  successors, from_fn}`, `core::ops::{Bound, RangeBounds}`, and slices'
  `get` and `get_mut` with ranges (rustc's `SliceIndex`),
  `chunks_exact_mut`, `rchunks`, `rchunks_exact` and `copy_within`.
  `str`'s searching methods take rustc's patterns: `&str`, `char`,
  arrays and slices of `char`s and closures, with `rfind`,
  `strip_prefix`, `strip_suffix`, `split_once`, `rsplit_once`,
  `trim_matches` and its one-sided forms, `rsplit`, `split_terminator`,
  `split_inclusive`, `splitn`, `rsplitn`, `matches`, `match_indices`
  and their reverse forms, matching as rustc's searchers do; `str::get`
  takes ranges, and `Bytes` is an `ExactSizeIterator`, with rustc's
  `size_hint`s for `Bytes`, `Chars` and `CharIndices`. Closures meet
  traits through blanket `impl` blocks, and a bound that is not met
  leaves its associated types errors rather than types to annotate.
- Add the traits of rustc's prelude that `core` lacked: `FromIterator`
  (with `Iterator::collect`, and impls for `()`, pairs of collections,
  and `Option` and `Result` of collections, stopping at the first `None`
  or `Err` as rustc's `GenericShunt` does), `Extend` (with `unzip` and
  `partition`), `Into`, `TryFrom` and `TryInto` (every integer
  conversion `From` lacks, with `TryFromIntError`, and `char`'s, with
  `CharTryFromError` and `TryFromCharError`), and `AsRef` and `AsMut`;
  `Sum` and `Product` of `Option`s and `Result`s, and `From<T> for
  Option<T>`. An associated type whose trait's arguments are not known
  yet waits for them, and a method's trait that an `impl` block meets
  but whose bounds do not hold reports the unmet bound (E0277), as
  rustc does.
- Add `core::hash` (`Hash`, `Hasher`, `BuildHasher`,
  `BuildHasherDefault`) and `#[derive(Hash)]`, whose values make the
  same calls of a hasher's methods as rustc's, `core::cell::{Cell,
  RefCell}`, inline constants (`const { .. }`, checked as constants
  are, which a repeat expression repeats whatever their type), and raw
  pointers to places: `&raw const` and `&raw mut` (parsed without a
  contextual keyword, as only there may `const` or `mut` follow an
  identifier; safe of a `static mut`) and `core::ptr::{addr_of,
  addr_of_mut}`, with rustc's E0745 and E0596.
- Make tuple structs' and tuple variants' constructors functions, as
  rustc's are: their paths are values of function item types
  (`.map(Some)`, `.map(Token::Num)`, `.map(Wrapper)`), and functions
  used only as values, `core`'s included, are compiled. Add
  `Option::flatten`.
- Give `core`'s newer types rustc's `Debug` and `Display`: `Cell`,
  `RefCell` (`<borrowed>` while lent mutably), `Ref`, `RefMut`,
  `BorrowError`, `BorrowMutError`, `TryFromIntError` (now holding an
  `IntErrorKind`, as rustc's), `CharTryFromError`, `TryFromCharError`,
  `BuildHasherDefault`, `iter::Empty` and `iter::Repeat`.
- Support `?Sized`: type parameters of functions, structs, traits and
  `impl` blocks are `Sized` unless bounded `?Sized` (in their list or
  the `where` clause), as in rustc, so `fn f<T: ?Sized>(x: &T)` and
  `impl<T: ?Sized + Tr> Tr for &T` take `str` and slices, and an `impl`
  block applies only where its implicit bounds hold (`&str: Tr` through
  `impl<T: Tr> Tr for &T` is E0277, which flux used to accept). Values,
  parameters, struct arguments and calls of unsized parameters are
  E0277; misplaced and duplicate relaxed bounds and `?Trait` other than
  `Sized` are rustc's errors (E0203). `core` relaxes the bounds rustc's
  does: comparisons and formatting of references, raw pointers' traits,
  `PhantomData`, and `Iterator` for `&mut I`.
- Support trait objects: `&dyn Tr`, `&mut dyn Tr` and `*const dyn Tr`,
  with generic traits (`dyn Tr<i32>`), associated types (`dyn
  Iterator<Item = u32>`) and the `Fn`-family traits (`dyn Fn(i32) ->
  i32`, called like a function). References and pointers become ones to
  trait objects at every coercion site and in casts, and to supertraits'
  objects (upcasting); methods, provided and supertraits' ones included,
  are called through vtables laid out as rustc's (drop, size, align, the
  supertraits' vtables, then each trait's methods), which code
  generation emits as data objects of function addresses, the methods
  in them using C's convention. Dyn compatibility (E0038), E0191,
  E0225, E0393, E0161, E0599 and "cannot be invoked on a trait object"
  are reported as rustc does, and the borrow checker requires what a
  value made a trait object holds to outlive the object's lifetime
  bound, its reference's. `core`'s `Iterator for &mut I` takes `I:
  ?Sized`, so `&mut dyn Iterator` iterates in `for` loops and adapters.
- Support array and slice patterns: fixed lengths, `..` with elements
  before and after it, and `rest @ ..`, in `match`, `let`, `if let`,
  `while let` and parameters, by value and through default binding
  modes, with rustc's E0527, E0528, E0529 and E0730, and exhaustiveness
  checked with rustc's slice constructors and witnesses.
- Make slice patterns' elements and rest places of their own, as
  rustc's MIR does (`ConstantIndex` and `Subslice` projections): the
  borrow checker tells apart elements at different constant indices,
  elements and the rest may be moved out of an array, which drops what
  is left as rustc's elaborated drops do (runs of elements, the last
  first), and `rest @ ..` binds by reference and mutably too. Moving
  out of a slice is E0508, and a place of unsized type may be matched
  (`match *s`).
- `core::fmt`'s `Formatter` writes through a `&mut dyn Write` and its
  `debug_*` builders take `&dyn Debug`, as rustc's do, with output
  unchanged; `fmt::write` takes a `&mut dyn Write`.
- Support inherent `impl dyn Tr` blocks, in the trait's crate (E0116
  elsewhere), and reach every method of a trait object's traits, so
  that `core`'s provided ones (`Write::write_char`, `Iterator::nth`)
  are built for vtables.
- Select a method's trait bounds before coercing its arguments, as
  rustc's confirmation does, so a trait's generic arguments that its
  only `impl` block for the receiver decides are known first:
  `a.partial_cmp(&b)` of `&str`s and `x.cmp(&y)` of `&[T]`s coerce the
  argument to `&str` and `&[T]`. (Where rustc's `alloc` adds `impl`
  blocks `core` lacks, such as `str: PartialEq<String>`, rustc finds
  several and coerces nothing, so `a.eq(&b)` of `&str`s is rejected by
  rustc and accepted by flux.)
- Fix a miscompile of statics and promoted constants of enums one of
  whose variants holds an array (`static G: E = E::B(8, 3)` beside
  `E::A([i32; 2])`, `Result<[i32; 2], i64>`): their data put another
  variant's fields where an array's address would be in registers, not
  where its elements are in memory, so code read them at other offsets.
  A constant's bytes are now written field by field at the offsets of
  the layout code reads and writes them with.
- Make `Zip` behave as rustc's specialized one, whose closures ran
  differently: `a.iter().map(f).zip(0..2).fold(..)` called `f` for a
  third item. `core` marks the iterators rustc's specializations trust
  (`TrustedLen`, `TrustedRandomAccess`) with hidden constants of
  `Iterator`: pairs of slice, range, `str::bytes` and chunk iterators
  and adapters of them are got by index in `next`, `nth`, `next_back`
  and `fold`, and `fold` of iterators that know their lengths takes as
  many pairs as the shorter has. `Zip` is a `DoubleEndedIterator` of
  exact-size iterators; `count`, `last`, `for_each`, `sum`, `product`,
  `max`, `min` and their forms, `partition`, `unzip` and collecting
  `()` fold, and adapters fold (and `rfold`) their inner iterators, as
  rustc's do; `eq` of iterators that know their lengths compares none
  of their items when the lengths differ; and flattening arrays has
  rustc's `size_hint`. The constants, `#[rustc_specialization]` in
  `core`, leave `Iterator` dyn compatible, as rustc's specializations
  do. An impl method's bound naming a type its block gives an
  associated type (`G: FnMut(Acc, B)` where `Item = B`) is no longer
  E0276 against the trait's (`F: FnMut(B, Self::Item)`).

- Match rustc's Unicode printability tables when escaping characters in
  `Debug` formatting, and preserve parentheses in panic source locations.
  Array `IntoIter` and `Skip`/`Rev` use the same skipping paths and drop
  order as rustc. Normalize both sides of impl method bounds containing
  associated types before comparing them. Differential tests also order
  `clamp` bounds using the generated integer type.
- Use the same trusted-length metadata for iterator specialization and
  `Vec` collection reservation, including owning vector iterators and
  drains, so `Zip` and iterator equality preserve rustc's closure calls.
- Support `core::ops::Deref` and `DerefMut`: `*w` on types that
  implement them, autoderef through them in method calls, field
  accesses and indexing (several levels deep, mixed with references),
  and deref coercion of `&W` to `&W::Target` (`&mut` through
  `DerefMut`), with the built-in deref coercion of references the same
  mechanism. `DerefMut` is chosen where the place is used mutably, as
  rustc's `convert_place_derefs_to_mutable` does, and the borrow
  checker reports rustc's E0594, E0596 and E0507 for writing through or
  moving out of a dereference ("cannot move out of dereference of
  `W`"), and now names overloaded indexing as rustc does ("cannot move
  out of index of `I`"). A block ending with a `&mut T` where a `&T` is
  wanted is reborrowed at its tail (`fn get(&self) -> &T { self.r }`
  moved `self.r`), and a trait `impl` block's method may assume what
  the trait's parameters imply for the block's types (`&'s Guard<'a,
  T>` that `'a: 's`), as rustc's `compare_impl_method` does.
- Lay values out in memory as rustc does (`flux_mir::layout`, rustc's
  `rustc_abi::layout` for aarch64): fields reordered by alignment and
  niche, tuples' last element last, `#[repr(C)]` (now on structs too)
  in order, enums tagged with the smallest integer their discriminants
  need or niche-filling (`Option<&T>` is one pointer, `Option<bool>` one
  byte), whichever is smaller. Code generation stores an enum's active
  variant and encodes its tag, and decodes it when loading; constants'
  data is laid out the same way. Add `core::mem::{size_of, align_of,
  size_of_val, align_of_val}` (in the prelude, `const`), `forget` and
  `MaybeUninit`, `core::ptr` with raw pointers' methods (`core` may now
  write `impl<T> *const T`), `NonNull` with its niche, and the coercion
  of `*mut T` to `*const T`. `tests/run/layouts.rs` checks 69 types'
  sizes against rustc, and a generator of random structs and enums
  agreed with rustc on 400 programs.
- Add flux's `alloc` library, built into the compiler after `core`:
  `alloc::alloc` over C's `malloc`, `realloc` and `free`, `Box`, `Vec`
  (growing as rustc's `RawVec` does, so `capacity()` agrees, dropping
  its elements in order) and `String`, with `vec!`, `ToString` and
  `ToOwned`, in the program's prelude as rustc's `std` prelude has
  them; `extern crate alloc;` names it. A failed import's name now
  shadows the prelude's, as rustc's dummy binding does, and `let v:
  Vec` is "missing generics for struct `Vec`", as in rustc. Overlapping
  `impl` blocks are told apart by their parameters' implicit `Sized`
  bounds (`impl<T: Clone> ToOwned for T` is not for `str`).
- `alloc` follow-ups: `collect` and `extend` of `Vec` and `String`
  (`FromIterator`, `Extend`) with rustc's capacities, which its
  `TrustedLen` specialization decides (iterators say it through a
  hidden `Iterator::__TRUSTED_LEN`), `format!`, `fmt::Write` for
  `String`, `ToString` for every `Display` type with rustc's
  specializations' capacities (a hidden `Display::__to_string_hint`),
  `Vec`'s `Index` and `IndexMut` and `#[track_caller]` where rustc's
  has them, so its panics are reported where rustc's are, `Vec::drain`,
  and moving out of a `Box`: `*b` is rustc's built-in box dereference
  (`ProjectionElem::BoxDeref`), its contents owned, and a box partly
  moved out of frees its memory through its `Unique` field. `Box` may
  hold unsized values: `Box<dyn Tr>` and `Box<[T]>` coerce from boxes
  of sized values, and drop and free through `drop_in_place`,
  `size_of_val` and `align_of_val`, now intrinsics for unsized types
  too (a slice's elements in a loop, a trait object through its
  vtable, whose drop slot holds `drop_in_place`'s instance).
  `format_args!` writes literals a plain `{}` shows into its string, as
  rustc does.
- Add `alloc`'s comparisons as rustc's has them: `String` with `str`
  and `&str` either way round, and `Vec<T>` with `Vec<U>`, `[U]`,
  `&[U]`, `&mut [U]`, `[U; N]` and `&[U; N]` (and slices and references
  to them with `Vec`). `str` and `[T]` now have several `PartialEq`
  impls, so `a.eq(&b)` of two `&str`s is rejected as in rustc ("can't
  compare `str` with `&str`", E0277 at the argument: a method's trait
  bound unmet for arguments its arguments gave is reported there), and
  `&str == String` uses the impl for the reference.
- Bound field and index autoderef to 128 steps, so cyclic or growing
  `Deref` targets report a recursion-limit error instead of hanging.
  Mark `Vec::IntoIter` as trusted-length so collecting its adapters
  reserves the known element count. Exercise partial boxed-array
  patterns, nested boxes, boxed slices and trait objects, drains and
  partially consumed iterators with repeated ownership transfers.
- Fix an internal compiler error for a closure matching an element of a
  captured array with only wildcards (`match p[0] { _ => .. }`, `let _ =
  p[i]`): as in rustc, indexing borrows the array whatever the patterns
  read (mutably for `ref mut`), the index evaluated after it, while
  wildcards alone (`[_, _]` of an array, `[..]` of a slice) read nothing
  and capture nothing. A match's arms are walked in order, pattern,
  guard and body.
- Capture what rustc's closure capture analysis does: a by-value use of
  a `Copy` part of an array's element (`p.1[1].0.is_positive()`, `let x
  = p[0].0`) borrows the array instead of moving it into the closure;
  with `move`, a value of a type with a `Drop` impl is moved whole for a
  part of it that is not `Copy`; a place is cut after its last
  dereference only if that is of a shared reference; what a raw pointer
  points to is borrowed shared even to change it (an `Fn` closure, the
  pointer needing no `mut`), and writing through a `*mut` needs nothing
  of the path to it. Captures are ordered by binding, in the order each
  is first used, and by field within one, so a closure drops them in
  rustc's order.
- Avoid reading an uncaptured single-variant enum when a closure's
  pattern only tests its variant and wildcard fields. Indexed captures
  also support `ref` and `ref mut` bindings inside field patterns.
- Allow borrowing a struct mutably while a borrow through one of its
  shared-reference fields is live, without relaxing conflicts through
  mutable references. Track function-item output lifetimes through
  iterator adapters using the function's signature, including invariant
  references; function-pointer outputs conservatively keep all argument
  lifetimes equal. This fixes false borrow errors in the JSON example.
- Preserve overloaded `Deref` borrows when analyzing indexed and copied
  closure captures, including deref coercions. Project capture types
  through `Box`, and select `DerefMut`/`IndexMut` for scrutinees bound by
  explicit `ref mut` patterns in `let`, `if let` and `match`.

- Resolve closure captures of deferred match-guard bindings through the
  scrutinee, avoiding false uninitialized-binding errors. Preserve guard
  move and mutation restrictions, including bindings of enclosing guards.

### Performance

- Guard against slowdowns worse than linear. The corpus tests fail on a
  file a stage spends too long on for the bytes it read (200 ms, or 100
  ns/B to parse, 500 to check trees against the grammar and 1000 to
  lower or type-check; the largest files take 6, 80, 50 and 50 alone),
  timed again alone before failing; `FLUX_CORPUS_FLOOR_MS` and
  `FLUX_CORPUS_NS_PER_BYTE` change the budget. New `scaling` tests in
  `flux-syntax`, `flux-hir`, `flux-ty` and `flux-mir` generate programs
  of size n and 4n (items, derives, `impl` blocks, supertraits, bounded
  associated types, modules, module files, long bodies, errors) and
  fail if the larger takes six times as long; with the derive, file
  lookup, `impl` type and elaboration fixes undone, they do. Ignored
  ones record what is still quadratic: names looked up through every
  binding in scope, `impl` blocks of one trait compared and searched
  pairwise, the traits in scope listed pairwise for each body, and the
  MIR checks' state per block and per statement.
- Fix the slowdowns the ignored `scaling` tests recorded, which now run:
  a local name is looked up through a map to its innermost binding, not
  through every binding in scope; the traits in scope are listed once
  per module, and a method lookup goes through those with its name when
  they are fewer; a trait's `impl` blocks are indexed by the outermost
  constructor of their type, as rustc's `SimplifiedType` has them, for
  goals and the overlap check, and a function's call targets are queued
  for checking once rather than at every call; and the borrow check and
  initialization check keep their dataflow state in bitsets per block,
  the borrow check finding the live locals at each statement by undoing
  each statement's changes rather than copying the set. Four times as
  many statements naming a parameter took 8.5 times as long to lower,
  and now 4.0; four times as many traits in scope took 22 times as long
  to check, `impl` blocks of one trait 13.6, now 3.7 and 3.8; a block
  four times as long took 13.3 times as long to build MIR for, a body
  branching at each statement 10.6, now 3.9 and 4.2. Diagnostics and
  snapshots are unchanged.
- Intern types without contention: a thread looks a type or a list of
  types up in its own cache first, then in one of 64 shards of the
  table, each with its own lock on its own cache line, rather than all
  threads taking one global lock for every type they build. A file whose
  arrays make thousands of types took 6 ms alone but 450 ms while 17
  other threads checked copies of it; now 9 ms. Type-checking the corpus
  on 18 threads takes about 40% less CPU time, and slightly less alone.
- Speed up the front end on one thread: about 245 MiB/s for whole-file
  parsing instead of 200 (`just bench`: lexing 6.3 to 5.4 ns/token,
  parsing 5.2 to 3.9, building 6.0 to 4.9). The parser's lookahead is
  inlined, so its test for composite tokens folds away for the constant
  kinds the grammar asks for; the tree builder no longer rewrites every
  event, nor counts the trivia before nodes that take no comments; the
  lexer finds keywords through a perfect hash of their bytes and no
  longer tracks the last two significant tokens for every token; and
  the token, input and node arrays are sized so that they do not grow.
- A file's text moves into its tree instead of being copied, paths in
  the item tree allocate each of their arrays once at its final size,
  the item tree's diagnostics are moved into the HIR's rather than
  cloned and deduplicated without copying their messages, and a crate's
  files are found by path through a map rather than a scan: windows'
  module tree (672 files, 114 MB) lowers in 1.3 s instead of 1.6 s.
  Type checking it takes 0.17 s instead of 0.80 s: resolving the type
  of each `impl` block of a trait looked through every diagnostic so
  far for those without a file, rather than through its own.

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
