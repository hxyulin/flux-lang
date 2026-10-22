# Supported language

flux accepts a subset of Rust, edition 2024. Anything flux accepts must be
valid Rust with the same meaning, so `rustc` can check flux's behavior.
This page covers syntax; later stages will narrow what is accepted further
(no `async` at first, for example) and will be documented here as they land.

## Syntax

**Items:** functions (including `const`, `unsafe`, `extern "ABI"`
qualifiers and bodiless declarations), structs (unit, tuple, record), enums
(with fields and discriminants), `impl` blocks (inherent and trait),
traits, type aliases (including generic associated types), `const` and
`static` items, inline and out-of-line modules, `use` trees, `extern crate`,
`extern` blocks (including C-variadic declarations), `macro_rules!`
definitions, and macro calls in item position. Visibilities: `pub`,
`pub(crate)`, `pub(self)`, `pub(super)`, `pub(in path)`.

**Attributes:** outer and inner attributes, with any token tree or a
`= expression` value, including edition 2024's `#[unsafe(...)]`.

**Generics:** lifetime, type and const parameters with bounds and defaults;
where clauses; higher-ranked `for<'a>` bounds; `?Sized`; parenthesized
`Fn(A) -> B` bounds; associated type arguments (`Item = T`, `Item: Bound`,
`Item<'a> = T`); `use<..>` precise capturing.

**Types:** paths (with generic arguments and qualified `<T as Trait>::X`
forms), references, raw pointers, arrays, slices, tuples, `!`, `_`, function
pointers, `impl Trait`, `dyn Trait`, and macro calls.

**Patterns:** bindings (`ref`, `mut`, `@`), wildcards, rest patterns,
literals (including negative numbers), inclusive, exclusive and half-open
ranges, paths, tuple-struct, record, tuple, slice and reference patterns,
and or-patterns.

**Expressions:** literals, paths, all unary, binary, compound-assignment,
cast and range operators with Rust's precedence; calls, method calls
(with turbofish), fields, tuple indices, indexing, `?` and `.await`; tuples,
arrays (`[x; n]` too), struct literals with `..base`; blocks (including
`unsafe`, `const`, `async` and labeled blocks); `if`, `if let` and `let`
chains, `match` with guards, `loop`, `while`, `while let`, `for`, labeled
`break`/`continue` with values, `return`; closures (`move`, `async`, typed
parameters, return types); `_` in assignments; attributes on expressions.

**Statements:** `let` with type, initializer and `else`, items, expression
statements, following Rust's rule that a block-like expression ends a
statement (`if c {} - 1` is two statements).

## Beyond syntax

Everything above parses, but only this part compiles so far; the rest is
reported as "not supported yet":

- **Items:** functions, including `const fn`; structs (named-field,
  tuple and unit structs); enums, with unit, tuple and named-field
  variants, generic ones included, and discriminants (`enum E { A = 1,
  B, C = 1 << 4, D = N + f() }`), constant expressions whose type
  `#[repr(u8)]` and the other integer types set, as `#[repr(C)]` may be
  written too (other `repr` hints are not supported yet); inherent `impl`
  blocks for them, with
  associated functions, methods (`self`, `mut self`, `&self`,
  `&mut self`) and constants; traits with required and provided methods
  and associated functions, associated types (`type Out: Bound;`),
  generic parameters (with defaults, `trait Add<Rhs = Self>`) and
  supertraits, and `impl` blocks of them for any type (`impl Tr for S`,
  `impl<T: Tr> Tr for W<T>`, `impl Tr for i32`, `impl Tr for &S`,
  blanket `impl<T: A> B for T`); type parameters on all of these, with
  trait bounds in their lists and `where` clauses (`fn f<T: A + B>`,
  `where T: Tr<u8>`), without defaults (`fn id<T>(x: T) -> T`,
  `struct Pair<T>`, `impl<T> Pair<T>`, `impl Pair<i32>`), structs
  included (`struct W<T: Tr>`, `where` clauses); associated constants
  in traits, with or without a default, in their `impl` blocks and in
  generic inherent blocks (`const N: u32;`, `const N: u32 = 1;`);
  constants (`const N: i32 = 5;`); statics (`static X: T = v;`,
  `static mut X: T = v;`), of types without references; type aliases (`type Pair<T> = (T,
  T);`), through which a struct's or enum's associated items, variants
  and struct expressions are named (`Alias::new()`, `Alias::A`,
  `Alias { .. }`), but not a tuple or unit struct's constructor
  (`Alias(1)`), and whose bounds are not enforced, as in rustc;
  modules, inline (`mod m { }`) and in
  files (`mod m;` as `m.rs` or `m/mod.rs`); `use` declarations, with
  groups, renames, globs, `self`, `super` and `crate`; and
  `unsafe extern "C"` blocks of function declarations, including
  C-variadic ones (`fn printf(format: *const i8, ...) -> i32;`). Items
  may be `pub`, `pub(crate)`, `pub(super)`, `pub(self)` or
  `pub(in path)`, and flux checks privacy as rustc does. An enum's
  variants are named through it (`E::A`, `Self::A`) or imported
  (`use E::*;`, `use E::{A, B};`).
- **Types:** paths (`i32`, `bool`, `Self`, `m::S`, `crate::m::S`,
  `Pair<i32>`, a type parameter `T`), function pointers (`fn(i32) ->
  i32`, `unsafe fn()`, but not `extern "C" fn`), associated types (`T::Out`,
  `Self::Out`, `<T as Tr>::Out`), tuples, `!`, references without
  lifetimes (`&T`, `&mut T`), raw pointers (`*const T`, `*mut T`),
  arrays (`[T; N]`, whose length is an integer literal or a path to a
  constant of type `usize`: `[u8; 4]`, `[u8; N]`, `[u8; m::N]`; other
  constant expressions are not supported yet), slices behind pointers
  (`&[T]`, `&mut [T]`), and `_` inside bodies (`let p: Pair<_> =
  ...`).
- **Expressions:** integer, boolean and C string (`c"..."`, `cr"..."`)
  literals, local and function names, tuples, arrays (`[a, b, c]`,
  `[x; N]` with `N` as in array types), struct expressions
  (`S { a: 1, b, ..base }`, `E::V { a: 1 }`), tuple struct and variant
  constructors (`P(1, 2)`, `E::V(1)`), unit structs and variants, field
  accesses (`s.a`, `t.0`, through references), indexing of arrays and
  slices with a `usize` (`a[i]`, `s[i]`, through references, also
  assigned to and borrowed: `a[i] = v`, `&mut a[i]`),
  paths to items in modules and associated items (`m::f`, `S::new`,
  `Self::X`, `i32::MAX`), trait items, functions and constants, through
  a type, a trait or both (`T::f`, `S::N`, `Tr::f`, `<T as Tr>::N`,
  `<T>::f`), generic arguments
  in paths and method calls
  (`id::<u8>`, `Pair::<i32>::new`, `x.m::<u8>()`), method calls,
  borrows (`&x`, `&mut x`) and dereferences (`*r`), blocks (including
  `unsafe`), `if`/`else`, `if let` and `let` chains (`if let Some(x) = a
  && x > 1 && let Ok(y) = b`), `match` with guards (`if let` guards
  too), `loop`, `while`,
  `while let`, unlabeled `break` (with a value) and `continue`,
  `return`, calls, unary and binary operators, assignments (to locals,
  their fields and elements), `as` casts between integers and from
  enums without fields, `?` on `Option` and `Result`, and the methods
  `CStr::as_ptr` and, of arrays and slices, `len` and `is_empty`.
- **Statements:** `let`, with a type, a value and `else` (`let Some(x) =
  o else { return };`).
- **Patterns:** `x`, `mut x`, `ref x`, `ref mut x`, `x @ pat`, `_`,
  tuples, structs and variants (`S { a, b: x, .. }`, `E::V(x, ..)`,
  `None`), with `..` rest patterns, integer and `bool` literals
  (`-1`, `true`), constants of integer types and `bool` (`N`,
  `i32::MAX`, `Self::N`), ranges whose ends are either (`1..=9`,
  `0..N`, `5..`, `..=i8::MIN`), or-patterns (`1 | 2`) and reference
  patterns (`&x`). Constants of other types in patterns, and array and
  slice patterns (`[a, b, ..]`), are not supported yet.
- **Attributes:** only `allow`, `warn`, `deny`, `expect`, `inline`,
  `cold`, `must_use` and `doc`, which are ignored, `derive` on structs
  and enums,
  and
  `#[diagnostic::on_unimplemented(message = "...")]` on traits, whose
  message reports their unmet bounds, as in rustc.

Type checking follows rustc for this subset: the integer types, `bool`,
tuples, arrays, slices, structs, enums and `!`; integer literals default to `i32`; no implicit
conversions; operators need operands of the same type, except shifts,
and work on integers and `bool` only (comparing tuples or pointers is not
supported yet), and on references to them, as below; `as` converts between integers and from `bool`. `isize` and `usize` are
64 bits. `c"..."` is a `&'static CStr`, and `.as_ptr()` gives the
`*const c_char` (`*const i8` on both targets) that C functions take. As
in rustc, arguments after a variadic function's `...` must be at least
32 bits wide or pointers: `u8`, `i16` and `bool` need a cast to
`c_int`/`c_uint` first.

Values of `Copy` types are copied: integers, `bool`, raw pointers,
shared references, tuples and arrays of `Copy` types, structs that
implement `Copy` and type parameters bounded by it. Other values are moved, and
flux reports uses after a move, including partial moves of fields, as
rustc's borrow checker does. `#[derive(Clone, Copy, PartialEq, Eq,
PartialOrd, Ord)]` implements those traits for a struct as rustc does
(for a generic one, where its parameters implement them), comparing
fields in order for `PartialOrd` and `Ord`; other derives are not
supported yet.

Values are dropped as in rustc: a binding at the end of its block, in
reverse order of declaration, then the function's parameters; a
temporary at the end of its statement (a block's tail expression's
before the block's bindings, as edition 2024 does), or at the end of the
block when a `let` borrows it (`let r = &make();`); a value about to be
overwritten by an assignment. Dropping a value runs its type's `Drop`
impl, if it has one, then drops its fields in order. A value moved out
is not dropped, one moved out on some paths only is dropped where it
was not, and a struct with some fields moved out drops the rest. As in
rustc, `drop` may not be called directly (E0040), fields may not be
moved out of a struct with a `Drop` impl (E0509), a `Drop` impl must be
for a struct, with distinct type parameters as its arguments (E0120,
E0366), and only the bounds the struct has or implies (E0367),
`Copy` is not for types with destructors (E0184), constants and
`const fn`s may not drop values whose `drop` runs code (E0493), and a
temporary may not be dropped while borrowed (E0716).
Structs, enums, tuples, arrays and slices cannot be passed to C yet.

An array's length is part of its type: `[i32; 3]` and `[i32; 2]` differ
("expected an array with a size of 3, found one with a size of 2").
A length written as a constant is evaluated while types are checked,
by the same evaluator as other constants, so its errors (E0080, a
cycle) stop type checking, as in rustc. A repeat expression's value
must be `Copy` unless the length is 0 or 1 or the value is a constant
(E0277), and its length may not name a local (E0435). A slice (`[T]`)
has no size of its own, so it exists only behind a pointer: a value,
binding, parameter or field of a slice type is E0277, as in rustc
(unsized struct fields are not supported yet). A reference to an array
becomes a reference to a slice where one is expected (`let s: &[i32]
= &a;`, `f(&mut a)`), as rustc's unsizing coercion does. An index must
be a `usize` (E0277, "the type `[i32]` cannot be indexed by `i32`");
indexing anything other than an array or slice is E0608. Indexing is
checked at run time: an index past the end panics, which aborts, and
one known at compile time to be past an array's end is rustc's
deny-by-default `unconditional_panic` ("this operation will panic at
runtime"), or E0080 in a constant. As in rustc, borrowing an element
borrows the array, whatever the index: `&mut a[0]` and `&mut a[1]`
conflict (E0499), and elements may not be moved out of (E0508).
`len` and `is_empty` are built into the compiler, as is the length of
an array, `Copy`, `Clone` and `PartialEq` (`==` and `!=` on arrays of
one length) of arrays whose elements implement them: rustc's `core`
writes these with const generics, which flux does not have yet. Other
methods of arrays and slices, comparisons with `<`, ranges as indices
(`&a[1..3]`) and arrays in `const fn`s through references are not
supported yet.

Generic items are checked once, in terms of their parameters, and
compiled once per set of arguments the program uses them with
(monomorphization). A type parameter has the methods and operators of
the traits its bounds name, and no fields; it is moved, never copied.
An associated type is what the `impl` block for its types says, or what
a bound says (`T: Tr<Out = i32>`, also inside a bound on an associated
type: `type N: Add<Output = i32>;`); otherwise, for a type parameter, it
is a type of its own, as in rustc. Its trait must hold wherever it is
named: `<T as Tr>::Out` needs `T: Tr` (E0277) in signatures, bounds,
`impl` headers, annotations and paths, reported where rustc reports it.
Blocks for different instances of a struct (`impl Pair<i32>` and
`impl Pair<u8>`) may define the same name, and the type a method is
called on picks one.

A struct's bounds must hold wherever it is named, and are not assumed
there, as in rustc: `fn f<T>(w: W<T>)` with `struct W<T: Tr>` needs
`T: Tr` too (E0277, or E0271 for `Tr<Out = i32>`), and so do `impl`
blocks for it, annotations and paths in bodies, and the values its
constructor and struct expressions are given. Its fields may name its
bounds' associated types (`made: S::Out`). `#[derive]` keeps its bounds.

An associated constant of a trait is the `impl` block's for the type it
is named on, or the trait's default, which may use the block's other
constants (`const DOUBLE: u32 = Self::SIDES * 2;`), and whose type
may be one of its associated types (`const N: Self::Repr;`). Like one of
a generic block, it has a value per type. As in rustc, one that code
without generic parameters names is evaluated, and its errors (such as
overflow) reported, whether that code is used or not; one that generic
code names is evaluated for the instances `main` reaches, so its errors
are reported only by `flux build`, as rustc reports them only in a full
build.

Methods are found as rustc finds them: on the receiver's type, then on
each type it dereferences to, the type's own methods before those of
traits, and those of traits only if the trait is in scope (defined or
imported in the module, or in the prelude) or bounds a type parameter.
A call of a generic function or method must meet its bounds, and an
`impl` block must define its trait's required methods with its
signatures; flux reports what rustc does where they do not (E0277,
E0599, E0046, E0053, ...), and conflicting `impl` blocks of a trait.
Calls of trait methods are static: each calls the method of the
`impl` block for its types, chosen when the program is compiled.
Operators on other types than integers and `bool` call the methods of
`core::ops`'s traits (`Add`, `AddAssign`, `Neg`, ...) and
`core::cmp::PartialEq` (`==`, `!=`) and `core::cmp::PartialOrd` (`<`,
`<=`, `>`, `>=`), which `core` implements for the integers and `bool`
too, so generic code can use them; `&a == &b` and `&a < &b` compare `a`
and `b`. There are no trait objects (`dyn Tr`) or
`impl Trait` types, and no generic associated types. `Sized` is, as a bound:
every type flux has is sized, but a trait's provided methods must say
`where Self: Sized` to take or hold `Self` by value, as in rustc.

A pattern that is not a binding sees through references, and then binds
by reference, as rustc's default binding modes do: `Some(x)` matching a
`&Option<i32>` binds `x: &i32`. Edition 2024's rules hold: `mut`, `ref`
and `&` may not be written where a pattern already borrows. `match`es
must be exhaustive (E0004) and `let` and parameter patterns irrefutable
(E0005), checked as rustc checks them, integer ranges included, and
reported with the patterns rustc gives for what is not covered
(`E::B(_)`, `6_u8..=u8::MAX`, `usize::MAX..`). Where patterns name
constants, those checks wait for the constants' values, so they come
with constant evaluation's errors, after type checking, but report
the same. Values are moved into
bindings once an arm's pattern and guard match; a guard sees the values
it would move by reference, and may not move them, change the value
matched (E0510) or write through `ref mut` bindings (E0594), as in
rustc; a guard that fails tries the next alternative of an or-pattern. A `match` tests what its arms look at, in
order, so moving out of one variant's field and using the enum again is
a use of a partially moved value, as in rustc; drops of enums with
moved-out fields drop what the variant they hold still has.
`#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]` works on
enums as on structs; `PartialOrd` and `Ord` order variants by their
discriminants, then the fields of the same variant, as rustc's do.

Operators work on references to integers and `bool` as in rustc,
which match ergonomics makes common (`match &p { (a, b) => a + b }`):
`&a + b`, `a + &b`, `&a + &b`, `a += &b`, `-&a` and `!&a` call
`core`'s impls of the operator traits for references, which give the
value's type; a shift takes any integer type on its right, by value or
by reference; `==` and `!=` compare what references point to (`&mut`
ones too), through `core`'s `impl PartialEq<&B> for &A`, and `<`, `<=`,
`>` and `>=` through `impl PartialOrd<&B> for &A` (and the one for two
`&mut`s), so generic code bounded by these traits takes references. As
in rustc, `<` wants the same kind of reference on both sides (E0308),
where `==` takes either, and `m < m` of a `&mut` borrows it mutably
(E0502). Where the operands are,
past one reference each, integers of different types, the error is
rustc's pair (`&i32 + u8` is E0277 and E0308), and the other unmet
combinations are E0277, E0369, E0368 and E0600 as in rustc. In
constants and `const fn`s, an operator that calls a trait's method is
an error, as rustc's are not stably `const`: E0658 for `core`'s impls,
E0015 for the program's.

`x?` is the value in `Some(x)` or `Ok(x)`; on `None` or `Err(e)`, the
function returns `None`, or `Err(From::from(e))`, dropping its locals
as `return` does. As in rustc, the function must return an `Option` for
an `Option` and a `Result` for a `Result`, whose error type converts
from `e`'s with `core::convert::From` (each type from itself, the
program's impls, and the integer conversions that lose nothing), and
the errors are rustc's E0277 messages for the `Try` and `FromResidual`
bounds its desugaring has, E0308 where the value has another type than
wanted, and E0572 and E0658 in constants and `const fn`s. `?` moves its
operand, as rustc's `Try::branch(x)` does. rustc implements `Try` for
`ControlFlow` and `Poll` too, which flux does not have.

Constants are evaluated at compile time, calls to `const fn`s included,
and overflow, division by zero or a panic (`None.unwrap()`) there is
an error, as in rustc. So are enums' discriminants, whether the enum is
used or not, with rustc's checks: a discriminant counted past its
type's maximum (E0370), a value assigned twice (E0081), and one that
needs itself (E0391). As in rustc, `E::A as i32` needs only `A`'s
discriminant, so `B = Self::A as isize + 1` works, but making a value
of the enum or matching on it, even in a `const fn` it calls, needs
them all. So is
arithmetic in ordinary code that rustc can tell overflows, such as
`let x: u8 = 255; x + 1` (its deny-by-default `arithmetic_overflow`
lint), even with overflow checks off.

A path to a function (`double`, `S::new`, `<S as Tr>::m`, `id::<u8>`)
not called is a value of the function's own item type, written by
rustc as `fn(i32) -> i32 {double}`, which holds nothing; it coerces to
a function pointer of its signature where one is wanted (an annotation,
an argument, a field, a return value, an `as` cast), and `if` or
`match` arms naming different functions coerce to one, as in rustc.
An `unsafe fn` coerces only to an `unsafe fn` pointer, and `extern`
functions not at all. Calling either type works as calling the
function does; through a pointer, in constants and `const fn`s, is an
error, as in rustc. Function items and pointers are `Copy`, and `core`
implements `Clone` for pointers of up to three parameters (rustc's for
any). Tuple structs' and variants' constructors are not values yet.

A static is one place in memory, initialized with its initializer's
value, evaluated at compile time as a constant's is (it may read other
statics, a `static mut`'s initial value included), and never dropped.
As in rustc, an immutable static may be read, borrowed (`&X`) and
passed by reference but not assigned or moved out of (E0594, E0507),
its type must be `Sync`, which rules out raw pointers (E0277), and a
`static mut` may be used only in `unsafe` code (E0133); edition 2024's
`static_mut_refs` lint, deny-by-default, rejects references to one,
explicit or made by a method call, so it is read and written by value.
A constant may read an immutable static's value, but not a `static
mut`'s (E0080).

Every program can use flux's own `core` library, which has the parts of
Rust's that flux supports so far: the integer types' constants (`MIN`,
`MAX`, `BITS`) and methods such as `abs`, `pow`, `wrapping_add`,
`count_ones` and `rotate_left`, `core::mem::drop` (also in the
prelude, as `drop`), `core::convert::{identity, From}`,
`core::marker::{Sized, Copy}`, `core::clone::Clone`, the operator
traits of `core::ops` with `Drop`, `core::cmp::{PartialEq, Eq,
PartialOrd, Ord, Ordering, max, min}`, `core::option::Option` and
`core::result::Result` (`Sized`, `Copy`, `Clone`, `Drop`, `PartialEq`,
`Eq`, `PartialOrd`, `Ord`, `From`, `Option`, `Some`, `None`, `Result`,
`Ok` and `Err` in the prelude). `Ord` gives every type that implements
it, the integers included, `cmp`, `max`, `min` and `clamp`, and
`Ordering` has `is_eq`, `is_ne`, `is_lt`, `is_gt`, `is_le`, `is_ge`,
`reverse` and `then` (`const`, as in rustc); `then_with` takes a
closure, and comes later. `From` converts each type
from itself, `bool` to the integers, and integers to the wider ones
that hold all their values, as in rustc; `Into` comes later. `Option` has `is_some`,
`is_none`, `unwrap`, `unwrap_or`, `ok_or`, `and`, `or` and `xor`;
`Result` has `is_ok`, `is_err`, `ok`, `err`, `unwrap_or`, `and` and
`or`; both implement `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`
and `Ord` as rustc's do. As in rustc, constants may call `is_some`, `is_none`, `unwrap`,
`is_ok` and `is_err`, and the others are reported as unstable there. Methods that take closures, or return references inside them
(`as_ref`), come later. Anything else in
`core` is reported as not supported yet, and so is `std`.

References may be parameters, temporaries, and `let` bindings without
`mut` that are given a value where they are declared. That is enough to
borrow-check them without lifetimes, with rustc's rules and messages
(non-lexical lifetimes, two-phase borrows for method receivers, borrows
of distinct fields). Anywhere else (in tuples, structs or other
references, as return values, including a generic function's
(`id(&x)`), in `mut` bindings) they are reported as not supported yet.
A reference may be passed to a generic function that returns none.

At run time flux matches rustc with `-C overflow-checks=off -C
panic=abort`: arithmetic wraps, shift amounts are masked to the type's
width, and division by zero or `MIN / -1` aborts. Programs call C through
`unsafe extern "C"` blocks; there is no standard library yet.

## Deliberately rejected

- **Pre-2024 editions.** `gen` and `try` are reserved keywords, so
  `rng.gen()` and `try!(...)` are errors, as in rustc 2024. Old code needs
  `r#gen`.
- **Struct literals in conditions** (`if x == S { a: 1 } {}`): rejected as
  in rustc; parenthesize them. As in rustc, one whose `{` is followed by
  `name:` is reported once and read as a struct literal.
- **Comparing a cast with `<`** (`x as usize < y`): the `<` starts generic
  arguments, as in rustc. `<=` and other operators work.

## Not supported yet

- Contextual keywords: `union`, `auto trait`, `default fn`, `safe` items in
  `extern` blocks, `&raw const`. The parser sees only token kinds, so these
  need the lexer to mark candidate identifiers first.
- Negative impls (`impl !Send for T`), `box` patterns, `yield` and `gen`
  blocks, `try` blocks.
- Shebang lines and frontmatter.
- Validation that rustc does after parsing: escape sequences in literals,
  suffixes, attribute placement, `let` outside conditions.
