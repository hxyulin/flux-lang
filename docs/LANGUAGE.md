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
  tuple and unit structs); inherent `impl` blocks for them, with
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
  constants (`const N: i32 = 5;`); modules, inline (`mod m { }`) and in
  files (`mod m;` as `m.rs` or `m/mod.rs`); `use` declarations, with
  groups, renames, globs, `self`, `super` and `crate`; and
  `unsafe extern "C"` blocks of function declarations, including
  C-variadic ones (`fn printf(format: *const i8, ...) -> i32;`). Items
  may be `pub`, `pub(crate)`, `pub(super)`, `pub(self)` or
  `pub(in path)`, and flux checks privacy as rustc does.
- **Types:** paths (`i32`, `bool`, `Self`, `m::S`, `crate::m::S`,
  `Pair<i32>`, a type parameter `T`), associated types (`T::Out`,
  `Self::Out`, `<T as Tr>::Out`), tuples, `!`, references without
  lifetimes (`&T`, `&mut T`), raw pointers (`*const T`, `*mut T`), and
  `_` inside bodies (`let p: Pair<_> = ...`).
- **Expressions:** integer, boolean and C string (`c"..."`, `cr"..."`)
  literals, local and function names, tuples, struct expressions
  (`S { a: 1, b, ..base }`), tuple struct constructors (`P(1, 2)`) and
  unit structs, field accesses (`s.a`, `t.0`, through references),
  paths to items in modules and associated items (`m::f`, `S::new`,
  `Self::X`, `i32::MAX`), trait items, functions and constants, through
  a type, a trait or both (`T::f`, `S::N`, `Tr::f`, `<T as Tr>::N`,
  `<T>::f`), generic arguments
  in paths and method calls
  (`id::<u8>`, `Pair::<i32>::new`, `x.m::<u8>()`), method calls,
  borrows (`&x`, `&mut x`) and dereferences (`*r`), blocks (including
  `unsafe`), `if`/`else`, `loop`, `while`, unlabeled `break` (with a
  value) and `continue`, `return`, calls, unary and binary operators,
  assignments (to locals and their fields), `as` casts between
  integers, and one method: `CStr::as_ptr`.
- **Patterns:** `x`, `mut x`, `_`, tuples, structs (`S { a, b: x, .. }`),
  tuple structs and unit structs, with `..` rest patterns. Patterns are
  irrefutable: `let` and parameters only.
- **Attributes:** only `allow`, `warn`, `deny`, `expect`, `inline`,
  `cold`, `must_use` and `doc`, which are ignored, `derive` on structs,
  and
  `#[diagnostic::on_unimplemented(message = "...")]` on traits, whose
  message reports their unmet bounds, as in rustc.

Type checking follows rustc for this subset: the integer types, `bool`,
tuples, structs and `!`; integer literals default to `i32`; no implicit
conversions; operators need operands of the same type, except shifts,
and work on integers and `bool` only (comparing tuples or pointers is not
supported yet); `as` converts between integers and from `bool`. `isize` and `usize` are
64 bits. `c"..."` is a `&'static CStr`, and `.as_ptr()` gives the
`*const c_char` (`*const i8` on both targets) that C functions take. As
in rustc, arguments after a variadic function's `...` must be at least
32 bits wide or pointers: `u8`, `i16` and `bool` need a cast to
`c_int`/`c_uint` first.

Values of `Copy` types are copied: integers, `bool`, raw pointers,
shared references, tuples of `Copy` types, structs that implement
`Copy` and type parameters bounded by it. Other values are moved, and
flux reports uses after a move, including partial moves of fields, as
rustc's borrow checker does. `#[derive(Clone, Copy, PartialEq, Eq)]`
implements those traits for a struct as rustc does (for a generic one,
where its parameters implement them); other derives are not supported
yet.

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
Structs and tuples cannot be passed to C yet.

Generic items are checked once, in terms of their parameters, and
compiled once per set of arguments the program uses them with
(monomorphization). A type parameter has the methods and operators of
the traits its bounds name, and no fields; it is moved, never copied.
An associated type is what the `impl` block for its types says, or what
a bound says (`T: Tr<Out = i32>`, also inside a bound on an associated
type: `type N: Add<Output = i32>;`); otherwise, for a type parameter, it
is a type of its own, as in rustc.
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
`core::cmp::PartialEq` (`==`, `!=`), which `core` implements for the
integers and `bool` too, so generic code can use them; `&a == &b`
compares `a` and `b`. There are no trait objects (`dyn Tr`) or
`impl Trait` types, no generic associated types, and `PartialOrd` (so
`<` on other types) is
not supported yet, nor are operators on
references to integers (`&a + &b`, `-&a`). `Sized` is, as a bound:
every type flux has is sized, but a trait's provided methods must say
`where Self: Sized` to take or hold `Self` by value, as in rustc.

Constants are evaluated at compile time, calls to `const fn`s included,
and overflow or division by zero there is an error, as in rustc. So is
arithmetic in ordinary code that rustc can tell overflows, such as
`let x: u8 = 255; x + 1` (its deny-by-default `arithmetic_overflow`
lint), even with overflow checks off.

Every program can use flux's own `core` library, which has the parts of
Rust's that flux supports so far: the integer types' constants (`MIN`,
`MAX`, `BITS`) and methods such as `abs`, `pow`, `wrapping_add`,
`count_ones`, `rotate_left`, `min` and `max`, `core::mem::drop` (also in
the prelude, as `drop`), `core::convert::identity`,
`core::marker::{Sized, Copy}`, `core::clone::Clone`, the operator
traits of `core::ops` with `Drop`, and `core::cmp::{PartialEq, Eq}`
(`Sized`, `Copy`, `Clone`, `Drop`, `PartialEq` and `Eq` in the
prelude). Anything else in
`core` is reported as not supported yet, and so is `std`.

References may be parameters, temporaries, and `let` bindings without
`mut` that are given a value where they are declared. That is enough to
borrow-check them without lifetimes, with rustc's rules and messages
(non-lexical lifetimes, two-phase borrows for method receivers, borrows
of distinct fields). Anywhere else (in tuples, structs or other
references, as return values, including a generic function's
(`id(&x)`), in `mut` bindings) they are reported as not supported yet,
and so are operators on references (`r + 1`; write `*r + 1`). A
reference may be passed to a generic function that returns none.

At run time flux matches rustc with `-C overflow-checks=off -C
panic=abort`: arithmetic wraps, shift amounts are masked to the type's
width, and division by zero or `MIN / -1` aborts. Programs call C through
`unsafe extern "C"` blocks; there is no standard library yet.

## Deliberately rejected

- **Pre-2024 editions.** `gen` and `try` are reserved keywords, so
  `rng.gen()` and `try!(...)` are errors, as in rustc 2024. Old code needs
  `r#gen`.
- **Struct literals in conditions** (`if x == S { a: 1 } {}`): rejected as
  in rustc; parenthesize them.
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
