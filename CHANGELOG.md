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
  `swap`, whose body the compiler makes, and `replace`; slices'
  iterator is `Clone`. `core` may name lifetimes in its bounds'
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
