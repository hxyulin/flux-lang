# Supported language

flux accepts a subset of Rust, edition 2024. Anything flux accepts must be
valid Rust with the same meaning, so `rustc` can check flux's behavior.
Package source graphs also accept a deliberately bounded
[edition-2021 subset](EDITION_2021.md); file commands default to 2024.
This page covers syntax; later stages will narrow what is accepted further
(no `async` at first, for example) and will be documented here as they land.

## Syntax

**Items:** functions (including `const`, `unsafe`, `extern "ABI"`
qualifiers and bodiless declarations), structs (unit, tuple, record), enums
(with fields and discriminants), `impl` blocks (inherent and trait),
traits, type aliases (including generic associated types), `const` and
`static` items, inline and out-of-line modules, `use` trees, `extern crate`,
`extern` blocks (including C-variadic declarations), `macro_rules!`
definitions, macro calls in item position, and nongeneric unions with concrete
Copy fields (bounded as described below). `union` remains a contextual
keyword, so identifiers and macros named `union` still work. Visibilities: `pub`,
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
  included (`struct W<T: Tr>`, `where` clauses), except that a
  struct's or enum's type parameters may have defaults (`enum
  ControlFlow<B, C = ()>`), which types that leave them out get, in
  terms of the arguments before them (`ControlFlow<i32>`), as in rustc;
  const parameters (below), without defaults; associated constants
  in traits, with or without a default, in their `impl` blocks and in
  generic inherent blocks (`const N: u32;`, `const N: u32 = 1;`);
  constants (`const N: i32 = 5;`); statics (`static X: T = v;`,
  `static mut X: T = v;`), of types without mutable references; type aliases (`type Pair<T> = (T,
  T);`), through which a struct's or enum's associated items, variants
  and struct expressions are named (`Alias::new()`, `Alias::A`,
  `Alias { .. }`), but not a tuple or unit struct's constructor
  (`Alias(1)`), and whose bounds are not enforced, as in rustc;
  items inside function bodies and constants' values, scoped to their
  block; modules, inline (`mod m { }`) and in
  files (`mod m;` as `m.rs` or `m/mod.rs`); `use` declarations, with
  groups, renames, globs, `self`, `super` and `crate`; and
  `unsafe extern "C"` blocks of function declarations, including
  C-variadic ones (`fn printf(format: *const i8, ...) -> i32;`). Items
  may be `pub`, `pub(crate)`, `pub(super)`, `pub(self)` or
  `pub(in path)`, and flux checks privacy as rustc does. An enum's
  variants are named through it (`E::A`, `Self::A`) or imported
  (`use E::*;`, `use E::{A, B};`).
- **Types:** paths (`i32`, `bool`, `char`, `Self`, `m::S`, `crate::m::S`,
  `Pair<i32>`, a type parameter `T`), function pointers (`fn(i32) ->
  i32`, `unsafe fn()`, but not `extern "C" fn`), associated types (`T::Out`,
  `Self::Out`, `<T as Tr>::Out`), `impl Trait` in functions'
  parameters and return types (below), tuples, `!`, references with or
  without lifetimes (`&T`, `&'a mut T`), raw pointers (`*const T`, `*mut T`),
  arrays (`[T; N]`, whose length is a constant expression of type
  `usize`: `[u8; 4]`, `[u8; N]`, `[u8; m::N]`, `[u8; N * 2 + 1]`,
  `[u8; double(N)]`, `[u8; Self::CAP]`, or a const parameter), slices
  and `str` behind
  pointers (`&[T]`, `&mut [T]`, `&str`, `*const [u8]`),
  `core::ffi::CStr` behind a reference, and `_` inside bodies (`let p:
  Pair<_> = ...`).
- **Expressions:** literals: integer, boolean, byte (`b'a'`), `char`
  (`'a'`, `'\u{1F600}'`), string (`"..."`, `r#"..."#`), byte string
  (`b"..."`, `br"..."`) and C string (`c"..."`, `cr"..."`), with their
  escapes checked as rustc's lexer checks them; local and function names, tuples, arrays (`[a, b, c]`,
  `[x; N]` with `N` as in array types), struct expressions
  (`S { a: 1, b, ..base }`, `E::V { a: 1 }`), tuple struct and variant
  constructors (`P(1, 2)`, `E::V(1)`), unit structs and variants, field
  accesses (`s.a`, `t.0`, through references), indexing of arrays and
  slices with a `usize` (`a[i]`, `s[i]`, through references, also
  assigned to and borrowed: `a[i] = v`, `&mut a[i]`) or a range of
  `usize` (`&a[1..3]`, `&mut s[lo..]`, `a[..=n]`, `&a[..]`), and of
  types that implement `core::ops::Index` (`grid[(r, c)]`),
  paths to items in modules and associated items (`m::f`, `S::new`,
  `Self::X`, `i32::MAX`), trait items, functions and constants, through
  a type, a trait or both (`T::f`, `S::N`, `Tr::f`, `<T as Tr>::N`,
  `<T>::f`), generic arguments
  in paths and method calls
  (`id::<u8>`, `Pair::<i32>::new`, `x.m::<u8>()`), method calls,
  borrows (`&x`, `&mut x`), raw pointers to places (`&raw const x`,
  `&raw mut s.f`, which make no reference, so they may point to a
  `static mut` outside `unsafe`, as `core::ptr::{addr_of, addr_of_mut}`
  do), dereferences (`*r`), blocks (including `unsafe`, and inline
  constants, `const { .. }`, checked as constants are, without the
  locals around them, and which a repeat expression may repeat whatever
  their type: `[const { Cell::new(0) }; 4]`; they are evaluated where
  they are, not at compile time, so one that panics or overflows is not
  an error, where rustc reports it), `if`/`else`, `if let` and `let` chains (`if let Some(x) = a
  && x > 1 && let Ok(y) = b`), `match` with guards (`if let` guards
  too), `loop`, `while`,
  `while let`, `for` loops, labels on loops (`'a: for`, `break 'a`,
  `continue 'a`) and blocks (`'a: { ... }`, `break 'a value`), `break` (with
  a value) and `continue`,
  `return`, calls, of functions, closures and values of types bounded
  by the `Fn`-family traits (`f(x)`, `(self.f)(x)`), unary and binary
  operators, ranges (`a..b`, `a..=b`, `a..`, `..b`, `..=b`, `..`),
  closures (`|x| x + 1`, `move |x: i32| -> i32 { .. }`; not `async`
  ones, nor ones returning references), assignments (to locals,
  their fields and elements), `as` casts (below), `?` on `Option` and
  `Result`, and the methods `CStr::as_ptr` and, of arrays and slices,
  `len` and `is_empty`, built into the compiler.
- **Statements:** `let`, with a type, a value and `else` (`let Some(x) =
  o else { return };`), and items.
- **Patterns:** `x`, `mut x`, `ref x`, `ref mut x`, `x @ pat`, `_`,
  tuples, structs and variants (`S { a, b: x, .. }`, `E::V(x, ..)`,
  `None`), with `..` rest patterns, integer, byte, `char`, `bool` and
  string literals (`-1`, `b'a'`, `'x'`, `true`, `"add"`), constants of
  integer types, `char` and `bool` (`N`, `i32::MAX`, `Self::N`), ranges
  whose ends are either (`1..=9`, `0..N`, `5..`, `..=i8::MIN`,
  `'a'..='z'`), or-patterns (`1 | 2`), reference patterns (`&x`), and
  array and slice patterns (`[a, b]`, `[first, .., last]`, `[x, rest @
  ..]`), with rustc's errors for lengths an array cannot have (E0527,
  E0528, E0730) and values that are neither (E0529), and exhaustiveness
  checked as rustc checks it (a slice's lengths up to the longest a
  pattern names, then the longer ones together: `&[_, _, ..]`). `rest
  @ ..` binds the elements it stands for, an array or a slice, by
  value or by reference. As in rustc, a slice pattern's elements and
  rest are places of their own (constant indices from either end and
  subslices), which the borrow checker tells apart where their indices
  differ (`[first, .., last]` of a `&mut [T]` binds both mutably), and
  an array's elements may be moved out of it one by one, what is left
  of it dropped in rustc's order; moving out of a slice is E0508.
  Constants of other types in patterns and byte string literals are
  not supported yet.
- **Attributes:** outer `#[cfg(predicate)]` on ordinary items (including
  modules, imports and item macros), associated items in traits and impls,
  and declarations in extern blocks. Multiple attributes are conjunctive;
  inactive items are removed before expansion, name resolution or lowering,
  including loading out-of-line modules. Predicates share `cfg!`'s evaluator:
  `all`, `any`, `not`, `debug_assertions`, `panic = "abort"`, `unix`,
  `windows`, and the target's `target_os`, `target_family`, `target_arch`,
  `target_pointer_width`, `target_endian` and `target_vendor`. The fixed
  backend baseline currently recognizes `target_feature = "neon"` on AArch64 and
  `"fxsr"`, `"sse"`, `"sse2"` on x86-64. Other feature predicates are false;
  flags used to build Flux do not change this configuration. `test`,
  features and unknown predicates are false; there are no build flags or
  test builds yet. Nested `cfg_attr` applies its selected attributes before
  semantic consumers inspect them. Block tails are selected after cfg pruning;
  explicit semicolons and remaining item statements still prevent a tail value.
  Inner file/module `cfg` and conditional
  statements, list elements and match arms are pruned before lowering.
  Conditional fields, variants and parameters, and expressions whose removal
  would leave a required operand missing, remain unsupported.
  Also `allow`, `warn`, `deny`, `expect`, `inline`,
  `cold`, `must_use` and `doc`, which are ignored, `derive` on structs
  and enums,
  and
  `#[diagnostic::on_unimplemented(message = "...")]` on traits, whose
  message reports their unmet bounds, as in rustc.

Type checking follows rustc for this subset: the integer types, `bool`,
`char`, `str`, tuples, arrays, slices, structs, enums and `!`; integer
literals default to `i32`; no implicit conversions; operators need
operands of the same type, except shifts, and work on integers and
`bool` only, with comparisons also available for `char` and thin raw pointers
(tuples compare through `core`'s impls, as below), and on references
to them, as below. `isize` and
`usize` are 64 bits. Thin raw pointers support all six comparisons with
unsigned address ordering and compatible pointee/mutability coercions.
Wide pointers, comparisons through references to pointers, and const pointer
comparisons remain unsupported. `c"..."` is a `&'static CStr` (`core::ffi::CStr`),
and `.as_ptr()` gives the `*const c_char` (`*const i8` on both targets)
that C functions take. `core` gives `CStr` rustc's `PartialEq` (with
`CStr` and `&CStr`), `Eq`, `PartialOrd` and `Ord`, which compare its
bytes, and `count_bytes`, `is_empty`, `to_bytes`, `to_bytes_with_nul`,
`to_str`, `from_bytes_with_nul` and `from_bytes_until_nul` (with
`FromBytesWithNulError` and `FromBytesUntilNulError`), which are not
`const` yet, as rustc's are: a `&CStr` is a pointer to its first byte,
where rustc's has its length too, so the length is counted where it is
needed. As in rustc, arguments after a variadic
function's `...` must be at least 32 bits wide or pointers (raw
pointers, or references to sized values): `u8`, `i16` and `bool` need a
cast to `c_int`/`c_uint` first, and a function item one to a function
pointer; `char`s pass as they are. Other types, which rustc passes as
they are (`&str`), are not supported yet.

`as` converts as rustc does: between integers, from `bool`, `char` and
fieldless enums to integers, and from `u8` (only, E0604) to `char`; a
reference to a raw pointer to what it points to (`&mut` to `*mut` or
`*const`, `&` to `*const` only) or, for an array, to its first element;
raw pointers to each other (a pointer to a slice or `str` keeps its
length, or drops it as a thin pointer; a thin one cannot become wide,
E0607); and raw pointers to sized types to and from integers. The others
are rustc's E0606 or E0054. A reference also coerces to a raw pointer
where one is wanted (`let p: *const i32 = &x;`).

A byte literal (`b'a'`) is a `u8`, a byte string (`b"ab"`) a `&'static
[u8; 2]`, which coerces to `&[u8]`, and a `char` a Unicode scalar value,
which exhaustiveness checking knows excludes the surrogates. A string
literal is a `&'static str`; `str` is unsized like `[u8]`, its UTF-8
bytes. `core` gives `u8` and `[u8]` their ASCII methods, `char` those
that need no Unicode tables (`to_digit`, `from_digit`, `from_u32`,
`len_utf8`, `is_whitespace`, the ASCII ones), and `str` `len`,
`as_bytes`, `as_ptr`, `bytes`, `chars`, `char_indices` (`Bytes` is an
`ExactSizeIterator`, and all three are iterators from both ends, with
rustc's `size_hint`s), comparisons, `str::from_utf8`, indexing by
ranges of bytes (`&s[1..4]`, which panics, as rustc's does, unless they
are within the string and at character boundaries, and `get`, which
gives `None` there) and the methods that give parts of it: `split_at`,
`trim`, `trim_start`, `trim_end`, `lines` and `split_whitespace`, with
`char::encode_utf8`. Its searching methods take rustc's patterns: a
`&str` (an empty one matching at every character boundary), a `char`, a
`[char; N]`, `&[char; N]` or `&[char]` (any of its characters), or a
closure or function `FnMut(char) -> bool` (the characters it is `true`
for): `starts_with`, `ends_with`, `contains`, `find`, `rfind`,
`strip_prefix`, `strip_suffix`, `split_once`, `rsplit_once`,
`trim_start_matches`, `trim_end_matches`, `split`, `rsplit`,
`split_terminator`, `split_inclusive`, `splitn`, `rsplitn`, `matches`,
`rmatches`, `match_indices` and `rmatch_indices`, whose matches and
parts are rustc's, overlapping `&str` matches included; as in rustc,
`trim_matches` and the iterators' back ends take only the patterns of
single characters (not a `&str`).
`char`'s Unicode-table methods (`is_alphabetic`, `to_uppercase`) are
not supported yet. Programs print with `println!` and the other
printing macros (below), or through C.

Values of `Copy` types are copied: integers, `bool`, raw pointers,
shared references, tuples and arrays of `Copy` types, structs that
implement `Copy` and type parameters bounded by it. Other values are moved, and
flux reports uses after a move, including partial moves of fields, as
rustc's borrow checker does. `#[derive(Clone, Copy, PartialEq, Eq,
PartialOrd, Ord, Default, Debug, Hash)]` implements those traits for a struct
as rustc does (for a generic one, where its parameters implement them;
one with lifetime parameters, for every lifetime),
comparing fields in order for `PartialOrd` and `Ord`, and making each
field's default for `Default`, and hashing an enum's discriminant (of
its `repr` type, `isize` by default, if it has more than one variant)
then each field for `Hash`. `Debug` writes what rustc's expansion writes
(`Point { x: 1, y: 2 }`, `Pair(1, "a")`, `Unit`, and `{:#?}` over several
lines), and a field that does not implement it is "`T` doesn't implement
`Debug`", at the field.

Macro calls are expanded where they are written: in items, statements,
expressions, patterns and types. `macro_rules!` macros work as in rustc:
rules tried in order, matched as rustc's macro parser matches them
(fragments `expr`, `ident`, `ty`, `pat`, `pat_param`, `tt`, `literal`,
`block`, `stmt`, `path`, `lifetime`, `vis`, `item` and `meta`,
repetitions `$(..)*`, `+` and `?` with separators, nested), with rustc's
errors ("no rules expected `x`", "unexpected end of macro invocation",
local ambiguity, a fragment followed by what its kind does not allow,
"recursion limit reached while expanding `m!`" past 128 nested
expansions). A macro is in scope after its definition, to the end of
its module or block (and after the module, with `#[macro_use]`), and
everywhere as `crate::m!` with `#[macro_export]`; `use m;` makes it
nameable by path (`crate::a::m!`, `self::m!`), as rustc's 2018 idiom
`pub(crate) use m;` does. In source dependency graphs, exported macros may
be renamed or reexported transitively; `$crate` retains the defining crate's
identity, including when captured tokens are forwarded or a macro generates
another macro. It does not bypass privacy and is distinct from the literal
`crate` token or an `ident` fragment. Item collection remains sequential;
macro lookup requiring later imports or a general expansion fixed point may
be rejected. Body macros can generate block-scoped `use` items, including
inside nested expansions; other generated local item kinds remain unsupported.
If later imports change a body macro's selected definition after collection,
Flux rejects the call instead of reusing its earlier expansion.
Hygiene is rustc's mixed-site hygiene: a local variable or label
a macro's definition writes is not the caller's, and the reverse,
except that a macro defined in a function sees the locals in scope at
its definition. Items a macro defines are found by name, from anywhere.
`core` has rustc's `assert!`, `assert_eq!`, `assert_ne!`,
`debug_assert!`, `debug_assert_eq!` and `debug_assert_ne!` (on: flux
compiles as rustc does without `-O`), `panic!`, `unreachable!`,
`todo!`, `unimplemented!`, `matches!`, `format_args!`, `write!`,
`writeln!`, `concat!`, `stringify!`, `line!`, `column!`, `file!`,
`module_path!`, `cfg!` (`debug_assertions`, `panic`, the target's
`target_os` and the like) and `compile_error!`, and `std`'s `print!`,
`println!`, `eprint!`, `eprintln!` and `dbg!`, which programs see as
rustc's see them in `std`'s prelude, by name or as `std::println!`
(not `core::println!`). Format strings are read and checked as rustc
reads them: positional, named and captured arguments, `{{` and `}}`,
fill, alignment, sign, `#`, `0`, width and precision (written, `1$`,
`name$` or `.*`), and the traits `Display`, `Debug` (with `x?` and
`X?`), `LowerHex`, `UpperHex`, `Octal`, `Binary`, `LowerExp`,
`UpperExp` and `Pointer`, each argument implementing its
placeholders', with rustc's errors where rustc reports them. Output is
byte for byte rustc's. stdout is line-buffered as std's is (what is
left without a newline is written when `main` returns, and lost if the
program panics); stderr is not. A panic prints std's message to stderr
(`thread 'main' (id) panicked at file:line:column:`, the message, and
the note about `RUST_BACKTRACE`) and aborts; the location is where
rustc's is, through `#[track_caller]` functions (`unwrap`, `expect`,
indexing, `clamp`, and the program's own), with the file named as it
was given to the compiler, except that code a `macro_rules!` macro
defines reports where it is written rather than the macro's call.
Constant evaluation reports a literal message, and `panic!("{}", s)`
of a `&str`, as rustc does. `let args = format_args!(..);` (rustc
keeps the arguments alive for the `let`), `format!`, `vec!`, `env!`,
`include!` and the like, items a macro makes inside a function body,
and calls in item position of macros named through modules (`a::m!();`
at a module's top level; `crate::m!` and `core::m!` work) are not
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
for a struct, with distinct type and lifetime parameters as its
arguments (E0120, E0366), and only the bounds, on types and lifetimes,
that the struct has or implies (E0367),
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
(E0277), and its length may not name a local (E0435). A length other
than a literal or a path is an anonymous constant, as in rustc: it may
not use generic parameters ("generic parameters may not be used in
const operations"; a repeat count that names a type parameter is
"constant expression depends on a generic parameter"), nor `Self`
where it is generic. A slice (`[T]`)
has no size of its own, so it exists only behind a pointer: a value,
binding, parameter or field of a slice type is E0277, as in rustc
(unsized struct fields are not supported yet). A reference to an array
becomes a reference to a slice where one is expected (`let s: &[i32]
= &a;`, `f(&mut a)`), as rustc's unsizing coercion does, and a
reference to a reference, or to a type that implements
`core::ops::Deref`, becomes the reference expected (`f(&&x)` for `f(x:
&T)`, `f(&w)` for a `w: Wrapper<T>`), as rustc's deref coercion does. An index must
be a `usize` or a range of them (E0277, "the type `[i32]` cannot be
indexed by `i32`"); indexing a type that does not implement
`core::ops::Index` is E0608. Other indexing than an array's or slice's
by a `usize` is `*Index::index(&a, i)`, or `*IndexMut::index_mut(&mut
a, i)` where the place is assigned to or borrowed mutably (E0594 and
E0596 without `IndexMut`), found on each type the base dereferences to
and then, for an array, on the slice it unsizes to, as in rustc; the
base is borrowed before the index is evaluated, without a two-phase
borrow, so `&mut s[..s.len() - 1]` is E0502 as in rustc. `core`
implements both for `[T]` with each range type, which panic as rustc's
do, with rustc's messages, and the program may implement them.
Indexing is checked at run time: an index past the end panics, and
one known at compile time to be past an array's end is rustc's
deny-by-default `unconditional_panic` ("this operation will panic at
runtime"), or E0080 in a constant. As in rustc, borrowing an element
borrows the array, whatever the index: `&mut a[0]` and `&mut a[1]`
conflict (E0499), and elements may not be moved out of (E0508).
`len` and `is_empty` are built into the compiler, as is the length of
an array. `core::array` implements, with const generics as rustc's
`core` does, `Copy`, `Clone` (a copy for `Copy` elements, as rustc's
specialization makes it, else each element's `clone` in turn),
`PartialEq` (`==` and `!=` on arrays of one length, and with slices
and references to them either way round: `s == [1, 2]`, `w == b"fn"`),
`Eq`, `PartialOrd`, `Ord` (`<` and the others, as slices compare) and
`Debug` of arrays whose elements implement them, `Default` of arrays
of up to 32 elements (any element type for none), and `IntoIterator`
for `[T; N]`, `&[T; N]` and `&mut [T; N]`; and `array::from_fn` and
arrays' `map`, `as_slice`, `as_mut_slice` and `each_ref` (`each_mut`
is not supported yet). `core::slice` gives slices `first`, `last` and
`get` (`Option<&T>` for an index, `Option<&[T]>` for any range of
them, through rustc's `SliceIndex`),
`iter` and `iter_mut`, iterators over references from both ends
(`Iter<'a, T>`, `IterMut<'a, T>`), and `IntoIterator` for `&[T]` and
`&mut [T]`, so `for x in slice` works, and `for x in &array` and `for
x in &mut array` iterate a slice of the array. `for x in array` and
`array.into_iter()` iterate it by value, with `core::array::IntoIter<T,
N>` (with `as_slice`), which moves the elements out one by one and
drops those it has not given when it is dropped. `core::slice` also
has `first_mut`, `last_mut`, `get_mut`, `split_first`, `split_last`,
`swap`, `reverse`, `rotate_left`, `rotate_right`, `split_at`,
`split_at_mut`, `copy_from_slice`, `clone_from_slice`, `fill`,
`contains`, `starts_with`, `ends_with`, `windows`, `chunks`,
`chunks_exact`, `chunks_mut`, `chunks_exact_mut` (with
`into_remainder`), `rchunks`, `rchunks_exact` (iterators from both
ends), `copy_within` (with any range, `core::ops::RangeBounds`), `binary_search`
(`_by`, `_by_key`, rustc's algorithm, so it finds the same one of equal
elements), `is_sorted`, and `sort`, `sort_by`, `sort_by_key` (in
rustc's `alloc`, which flux has none of) and their `sort_unstable`
forms: one stable merge sort that allocates nothing, so equal elements
keep their order, as rustc's unstable sort keeps them only up to 20
elements (it may order them differently beyond). Slices are
`PartialEq`, `Eq`, `PartialOrd` and `Ord`, comparing elements in turn
(`a[..2] == b[..2]`, `s < t`). The slice and array iterators are `ExactSizeIterator`s (see
`core::iter` below). Slices'' methods are found on arrays, and references to them,
as rustc's method probe finds them: the array is borrowed and the
reference unsized to a slice's (`arr.iter()`, `(&mut arr).last()`). Other
methods of arrays and slices, and arrays in `const
fn`s through references are not supported yet; neither is slicing in
constants, which rustc's `core` allows only unstably (E0658).

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

Const parameters (`struct Stack<T, const N: usize>`, `impl<T, const N:
usize> Stack<T, N>`, `fn sum<const N: usize>(a: [i32; N])`, `trait
Pick<const I: usize>`) are stable Rust's `min_const_generics`: of an
integer type, `bool` or `char` (other types are rustc's "`f32` is
forbidden as the type of a const generic parameter", and one naming
another parameter E0770). A parameter is a value of its type in
expressions (`N`), an array's length (`[T; N]`, `[x; N]`) and an
argument of other items (`Stack<T, N>`, `sum::<N>`), but not part of a
constant expression: `[T; N + 1]` is "generic parameters may not be
used in const operations", as `generic_const_exprs` is unstable, and a
pattern may not name one (E0158). Arguments are literals (`3`, `true`,
`'a'`), constants and parameters (`Stack<i32, LIMIT>`), blocks of
constant expressions (`{ N * 2 }`, `{ -1 }`), each an anonymous
constant of the parameter's type, `_`, or inferred (`sum([1, 2, 3])`
has `N = 3`; one that cannot be is E0284). A path argument is a type
if it names one, else a constant, as in rustc, and one of the wrong
kind is E0747; wrong counts are E0107, and arrays or instances of
different values are E0308 (`expected an array with a size of 3, found
one with a size of 4`, `expected `Stack<i32, 3>`, found `Stack<i32,
4>``). Each instance is monomorphized per value, as rustc does.

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

`core::ops::Deref` and `DerefMut` make a type dereference as a
reference does: `*w` is `*Deref::deref(&w)`, or `*DerefMut::deref_mut(&mut
w)` where it is assigned to, borrowed mutably, or the receiver of a
`&mut self` method, as rustc decides, and fields, methods and indexing
look through it (`w.x`, `w.len()`, `w[0]` for a `Deref<Target = [T]>`),
also several levels deep and mixed with references. As in rustc,
writing through a type without `DerefMut` is E0594 or E0596 ("cannot
assign to data in dereference of `W`"), moving out of one is E0507
("cannot move out of dereference of `W`", or "of index of `I`" for
`Index`), and a type that does not dereference is E0614.
Field and index autoderef stop after 128 steps with a recursion-limit
diagnostic if dereferencing continues, including cyclic `Deref` targets.

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
and `b`. There are no generic associated types. `Sized` is, as a bound and implicitly: every type
is sized but slices, `str` and trait objects, and a type parameter is
`Sized` unless bounded `?Sized` (`fn f<T: ?Sized>(x: &T)`, `impl<T:
?Sized + Tr> Tr for &T`), as in rustc; a trait's `Self` is not, so its
provided methods must say `where Self: Sized` to take or hold `Self` by
value. A value, parameter or struct argument whose size is unknown is
E0277, and so is a call or `impl` block that needs a parameter to be
`Sized` with an unsized type. A struct's last field may not be of an
unsized parameter's type yet (unsized structs are not supported), and
`?Sized` on associated types and `impl Trait` is not supported yet.

Trait objects (`dyn Tr`, `dyn Tr<i32>`, `dyn Iterator<Item = u32>`,
`dyn Fn(i32) -> i32`) exist behind references and raw pointers (`&dyn
Tr`, `&mut dyn Tr`, `*const dyn Tr`), each a pointer to the value and
one to its type's vtable. A reference or pointer to a value of a sized
type that implements the trait becomes one to a trait object at every
coercion site and in casts (`&x as &dyn Tr`), and one to a trait object
becomes one to a supertrait's (upcasting); the type must implement the
trait with the associated types the object gives (E0277, E0271) and be
sized. A trait object implements its trait and supertraits, so their
methods are found on it, provided ones included, and called through
its vtable; `dyn Fn`-family objects are called like functions. Its
trait must be dyn compatible, as rustc's rules say (E0038: no `Self:
Sized` supertrait, no `Self` as a supertrait's parameter, no
associated constants, and each method without `where Self: Sized`
takes `&self` or `&mut self` (or `self`, which cannot be called on a
trait object, E0161), has no type parameters, and names `Self` only in
its receiver or as `Self::Item`); every associated type must be given
(E0191) and every parameter whose default names `Self` (E0393); other
traits than the first are rejected (including extra auto-trait bounds,
which remain unsupported on trait objects), and a bare
trait as a type is E0782. A method with `where Self: Sized` "cannot be
invoked on a trait object". A trait object's lifetime bound (`dyn Tr +
'a`) is, unless written, rustc's default: the reference it is behind,
`'static` elsewhere. What a value made a trait object holds must
outlive it, a bound may only shorten (behind `&mut` too, as rustc's
unsizing coercion allows), and a parameter's lifetime bound (`T:
'static`) holds of a trait object if its bound does (E0310, E0521, as
rustc reports them). References and trait objects in a trait object's
generic arguments, trait objects in constants and statics (which are
not `Sync` anyway), references to trait objects cast to raw pointers,
`Box<dyn Tr>` and `dyn Any` are not supported yet. A trait's crate may
write inherent `impl dyn Tr` blocks (E0116 elsewhere), whose methods
trait objects of it have.

Struct- and enum-level `#[non_exhaustive]` retains its defining crate through
aliases and reexports. Outside that crate, struct construction/update is
rejected, tuple/unit constructors are private, struct patterns require `..`,
and enum matches need a wildcard. Same-crate use remains exhaustive.
Variant-level attributes are explicitly unsupported.

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
`#[derive(Default)]` on an enum makes the unit variant marked
`#[default]`, without bounds on its parameters, with rustc's errors for
none, several, one with fields or with a value, and `#[default]`
without the derive.

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
operand, as rustc's `Try::branch(x)` does. On a `core::ops::ControlFlow<B,
C>`, `x?` is the `Continue` value, and returns the `Break` from a
function that returns a `ControlFlow<B, _>`, with rustc's errors for
the other combinations. `?` is built into the compiler for these three
types; `core` also has rustc's `Try` and `FromResidual` traits for them,
which `try_fold` and its relatives are bounded by, but which programs
cannot name, as they are unstable in rustc (`Poll`, the other type
rustc implements them for, is not supported).

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

Items declared in a block are visible throughout it, before their
declaration too, and in the blocks inside it, where they shadow outer
items of their names, as in rustc: the block is an anonymous module
whose names are looked up in the module around it. `self` and `super`
name that module, and a private item in a block is private to it. An
inner function sees none of its outer function's locals (E0434),
generic parameters or `Self` (E0401), and an inner constant no local
(E0435).

A path to a function (`double`, `S::new`, `<S as Tr>::m`, `id::<u8>`)
not called is a value of the function's own item type, written by
rustc as `fn(i32) -> i32 {double}`, which holds nothing; it coerces to
a function pointer of its signature where one is wanted (an annotation,
an argument, a field, a return value, an `as` cast), and `if` or
`match` arms or array elements naming different functions (or
closures that capture nothing) coerce to one, as in rustc.
An `unsafe fn` coerces only to an `unsafe fn` pointer, and `extern`
functions not at all. Calling either type works as calling the
function does; through a pointer, in constants and `const fn`s, is an
error, as in rustc. Function items and safe or unsafe pointers implement
`Copy` and `Clone` for every supported signature, without bounds on argument
or result types. Reification rejects fixed, owner-bound, bounded or repeated
input lifetimes rather than discarding their constraints; this also rejects
some valid Rust signatures. Named pointer lifetimes and extern pointer ABIs
remain unsupported, and elided reference outputs retain conservative checks.
A tuple struct's or tuple variant's constructor is a function
too, as in rustc: its path not called (`Token::Num`, `Some`, `Wrapper`)
is a value of its own function item type (`fn(i64) -> Token
{Token::Num}`), which `.map(Some)` passes and which coerces to a
function pointer.

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

Core's `Send` and `Sync` propagate structurally through supported fields,
arrays, tuples, closures and PhantomData. Shared references require `T: Sync`
for either marker; mutable references require `T: Send` for Send and `T: Sync`
for Sync. Raw pointers do not receive either marker automatically, and Cell
is not Sync. Explicit unsafe implementations replace structural propagation,
including their generic obligations; shared statics use the same Sync rules.
Lifetime specialization/bounds in marker implementations and custom auto
traits remain unsupported. Recursive structural proofs are conservatively
depth-limited; no threading APIs are implied by these markers.

Every program can use flux's own `core` library, which has the parts of
Rust's that flux supports so far: the integer types' constants (`MIN`,
`MAX`, `BITS`) and methods: `abs`, `pow`, `count_ones`, `rotate_left`
and the others of bits and signs, the `checked_`, `wrapping_`,
`saturating_` and `overflowing_` arithmetic, `div_euclid` and
`rem_euclid`, `ilog2`, `ilog10`, `ilog`, `isqrt`, `midpoint`,
`next_power_of_two`, `div_ceil`, `next_multiple_of`, `swap_bytes`,
`reverse_bits` and the byte-order conversions (`to_le_bytes`, ...), all
`const` and panicking or overflowing in constants as rustc's do,
`core::fmt::{Debug, Display, Formatter, Arguments, Write, Result,
Error, Alignment, DebugStruct, DebugTuple, DebugList, DebugSet,
DebugMap, write}` as rustc's (`Debug` for the integers, `bool`,
`char`, `str`, `()`, raw pointers, references, slices, arrays, tuples,
`Option`, `Result`, `Ordering`, `CStr`, `Utf8Error` and the ranges;
`Display` for the integers, `bool`, `char`, `str`, `Arguments`,
`Utf8Error`, `Location` and references; `LowerHex`, `UpperHex`,
`Octal`, `Binary`, `LowerExp` and `UpperExp` for the integers, and
`Pointer` for pointers and references; `Formatter`'s `pad`,
`pad_integral`, `write_str`, `write_fmt`, the options' accessors and
the `debug_*` builders, whose `field` and `entry` take `&dyn Debug`,
as rustc's do), `core::panic::Location`
(`caller`, `file`, `line`, `column`),
`core::hint::black_box`, `core::hash::{Hash, Hasher, BuildHasher,
BuildHasherDefault}` (`Hash` for the integers, `bool`, `char`, `str`,
`()`, tuples, arrays, slices, references, raw pointers and `core`'s
types; each value makes the same calls of its hasher's `write_*`
methods as rustc's, a `str` its bytes then `0xff` through `write_str`,
a slice its length through `write_length_prefix` then its elements, all
at once for integers, so a program's hasher gives rustc's hashes),
`core::cell::{Cell, RefCell}` (a `RefCell`'s loans are counted, and
a conflicting `borrow` or `borrow_mut` panics with rustc's message, at
its caller; its `Ref` and `RefMut` cannot be dereferenced yet, as flux
has no `Deref`; all of them, and `BorrowError` and `BorrowMutError`,
are `Debug` as rustc writes them),
`core::ptr::{addr_of, addr_of_mut}`,
`core::default::Default` (in the prelude) for the integers, `bool`,
`char`, `&str`, `&[T]`, `&CStr`, `()`, `Option`, tuples of up to twelve elements and `Range`,
`core::mem::{drop, swap, replace, take}` (`drop` also in the prelude;
`swap` and `replace` are not `const` yet, as rustc's are),
`core::convert::{identity, From, Into, TryFrom, TryInto, AsRef, AsMut,
Infallible}`,
`core::marker::{Sized, Copy}`, `core::clone::Clone` (as `Copy`,
`PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Debug` and `Default`, for
tuples of up to twelve elements, comparing them element by element as
rustc's do: `==`, `<` and the others work on tuples, by reference too),
the operator
traits of `core::ops` with `Drop`, `Deref` and `DerefMut` (implemented
for references), `core::cmp::{PartialEq, Eq,
PartialOrd, Ord, Ordering, max, min}`, `core::option::Option` and
`core::result::Result`, `core::char` (`from_u32`, `from_digit`),
`core::str` (`from_utf8`, `Utf8Error`, `Chars`, `CharIndices`,
`Bytes`), `core::ffi` (`CStr`, `FromBytesWithNulError`,
`FromBytesUntilNulError`, `c_char`, `c_int` and C's other integer
types) (`Sized`, `Copy`, `Clone`, `Drop`, `PartialEq`,
`Eq`, `PartialOrd`, `Ord`, `From`, `Into`, `TryFrom`, `TryInto`,
`AsRef`, `AsMut`, `FromIterator`, `Extend`, `Option`, `Some`, `None`,
`Result`, `Ok` and `Err` in the prelude, as in rustc's 2024 prelude). `Ord` gives every type that implements
it, the integers included, `cmp`, `max`, `min` and `clamp`, and
`Ordering` has `is_eq`, `is_ne`, `is_lt`, `is_gt`, `is_le`, `is_ge`,
`reverse`, `then` (`const`, as in rustc) and `then_with`, with
`cmp::{max_by, min_by, max_by_key, min_by_key}`, and `cmp::Reverse`,
which orders the other way round (`sort_by_key(|x| Reverse(x.n))`).
`bool` has `then_some` and `then`. `From` converts each type
from itself, `bool` to the integers, and integers to the wider ones
that hold all their values, `u8` to `char` and `char` to `u32` and
wider, and `T` to `Option<T>`, as in rustc, and every type converts
`Into` those that convert from it. `TryFrom` (and `TryInto`, from it)
converts between every other two integer types, giving
`core::num::TryFromIntError` where the value does not fit (with its
`core::num::IntErrorKind`, `PosOverflow` or `NegOverflow`, shown as
rustc shows them; the enum is not `#[non_exhaustive]` yet, so a `match`
on it need not have a wildcard arm, which rustc requires), `u32` to
`char` (`core::char::CharTryFromError`) and `char` to `u8` and `u16`
(`TryFromCharError`), and, with `Infallible`, wherever `From` does.
`AsRef` and `AsMut` give slices of arrays and slices, `str` and its
bytes of a `str`, and see through references. `Option` has `is_some`,
`is_none`, `unwrap`, `expect`, `unwrap_or`, `ok_or`, `and`, `or` and
`xor`; `Result` has `is_ok`, `is_err`, `ok`, `err`, `unwrap`, `expect`,
`unwrap_err`, `expect_err`, `unwrap_or`, `and` and `or`, plus unsafe
`unwrap_unchecked` and `unwrap_err_unchecked`. These unchecked methods require
the corresponding variant and move its payload; using the wrong variant is
undefined behavior. They are not supported in constants. Both enums implement
`Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`
and `Ord` as rustc's do. As in rustc, constants may call `is_some`, `is_none`, `unwrap`,
`is_ok` and `is_err`, and the others are reported as unstable there.
Of the methods that take closures, `Option` has `map`, `map_or`,
`map_or_else`, `and_then`, `or_else`, `unwrap_or_else`, `filter`,
`ok_or_else`, `is_some_and`, `is_none_or`, `inspect` and `zip`, and
`Result` has `map`, `map_err`, `map_or`, `map_or_else`, `and_then`,
`or_else`, `unwrap_or_else`, `is_ok_and` and `is_err_and`. `Option`
also has `as_ref`, `as_mut`, `take`, `take_if`, `replace`, `insert`,
`get_or_insert`, `get_or_insert_with`, `get_or_insert_default`,
`unwrap_or_default` (as `Result` has) and `iter`, `copied` and `cloned`
of an `Option<&T>` (`const` where rustc's are, except `take` and
`replace`, which flux's constant evaluation cannot run yet), and is
`IntoIterator`, by value and by reference. Anything else in
`core` is reported as not supported yet. A bounded `std` facade
reexports implemented `core` and `alloc` APIs; see
[library/std/README.md](../library/std/README.md) for its scope.
Source library roots may use `#![no_std]`: they receive core's prelude,
without implicit std or alloc types or printing macros. Explicit `extern
crate alloc` and `extern crate std` opt into those libraries. Freestanding
executables remain unsupported; no_std executable roots require explicit
std. `#[rustfmt::skip]` is accepted as an inert tool attribute.

flux's `alloc` library has heap memory, through C's `malloc`, `realloc`
and `free` (`alloc::alloc::{alloc, alloc_zeroed, dealloc, realloc,
handle_alloc_error, Layout}`; failing to allocate aborts), and the
types that own it, which the program's prelude has as rustc's `std`
prelude does: `Box<T>` (`new`, `into_raw`, `from_raw`, `leak`,
dereferenced as rustc's built-in box is, so `*b` and its fields may be
moved out of, the rest dropped and the memory freed with the box, and
`*b` is mutable where `b` is; dropping it drops the value then frees
it; `Deref`, `DerefMut`, `Clone`, comparisons, `Debug`, `Display`,
`Default`, `From<T>`),
`Vec<T>` (`new`, `with_capacity`, `push`, `pop`, `insert`, `remove`,
`swap_remove`, `truncate`, `clear`, `retain`, `dedup`, `resize`,
`extend_from_slice`, `append`, `split_off`, `reserve`,
`reserve_exact`, `shrink_to_fit`, `shrink_to`, `len`, `capacity`,
`as_ptr`, `set_len`, `from_raw_parts`, `drain`; `Deref` to `[T]`, so
slices' methods work on it; `Index` and `IndexMut` by `usize` and
ranges, panicking at the brackets with rustc's messages, as `insert`,
`remove` and `split_off` do at their calls; `IntoIterator` by value,
by reference and mutably; `FromIterator` and `Extend`, so `collect`
makes one, with the capacities rustc's specializations give (exactly
the length of an iterator whose length is known, rustc's
`TrustedLen`, which flux's iterators say through a hidden method in
place of specialization; else room for the first item and the rest of
the lower bound); `Clone`, comparisons with vectors, slices and
arrays, `Debug`, `Default`, `From<&[T]>` and `From<[T; N]>`), whose
`capacity()` grows as rustc's `RawVec` grows it and whose elements drop
in order, `vec![a, b, c]` and `vec![x; n]`, and `String` (`new`,
`with_capacity`, `from`, `push`, `push_str`, `pop`, `insert`,
`insert_str`, `remove`, `truncate`, `clear`, `split_off`, `from_utf8`,
`into_bytes`, `as_str`, `Deref` to `str`, comparisons with `String`,
`str` and `&str`, `+` and `+=` with `&str`, `Clone`, `Debug`,
`Display`, `fmt::Write`, `FromIterator` and `Extend` of `char`s, `&str`s
and `String`s), with `format!` (rustc's capacity estimate, literals a
plain `{}` shows written into the format string as rustc does),
`ToString` for every `Display` type (with the room rustc's
specializations make: exact for text, the most digits of an integer),
`ToOwned` (`str` to `String`, `[T]` to `Vec<T>`), and slices' `to_vec`
and `repeat`. `Box<T>` may hold an unsized value: `Box<[T]>` and
`Box<dyn Tr>` from `Box<[T; N]>` and `Box<T>` where one is wanted
(rustc's `CoerceUnsized`), `Vec::into_boxed_slice`, `into_vec` and
`From<Vec<T>>`, dropped through the vtable's drop function and freed
with the size it records; `core::mem::{size_of_val, align_of_val}`,
`Layout::for_value` and `ptr::drop_in_place` take unsized values too.
Boxed slices support `Clone` and `FromIterator`; boxed strings support
`From<&str>` and `Clone`. `core::borrow::{Borrow, BorrowMut}` are also
reexported through `alloc::borrow` and `std::borrow`; String borrows as
`str`, and slices of `Borrow<str>` values support string `join`. String,
Vec and Box implement `Hash` when their contents do. `core::ptr::hash`
supports thin pointers only. `String::drain` and the other `alloc`
collections are not supported yet. Collecting an owning iterator does not
reuse its source allocation as rustc sometimes does, so capacities can differ in those
cases; a panic inside `alloc` that rustc reports at its own
source (`swap_remove`, `drain` out of range) is reported at flux's.

`core::ops` has the `Fn`, `FnMut` and `FnOnce` traits (in the
prelude), which bounds name as `Fn(A, B) -> C` (the angle-bracket
form, and parentheses on other traits, are errors, as in rustc), and
the range structs `Range`, `RangeInclusive`, `RangeFrom`, `RangeTo`,
`RangeToInclusive` and `RangeFull`, with `contains`, all `RangeBounds`
(with `Bound`, as `(Bound<T>, Bound<T>)` is too). `core::iter` has
`Iterator` (in the prelude) with `next` and the provided methods
`size_hint`, `rposition`, `count`, `last`, `nth`, `step_by`, `chain`, `zip`, `map`, `for_each`,
`filter`, `filter_map`, `enumerate`, `skip_while`, `take_while`,
`skip`, `take`, `fold`, `all`, `any`, `find`, `find_map`, `position`,
`max`, `min`, `max_by`, `min_by`, `max_by_key`, `min_by_key`, `rev`,
`sum`, `product`, `eq`, `size_hint`, `reduce`, `rposition`, `by_ref`,
`copied`, `cloned`, `flat_map`, `flatten`, `peekable` (with `peek`,
`peek_mut`, `next_if` and `next_if_eq`), `scan`, `map_while`,
`inspect`, `cycle` and `fuse`; `DoubleEndedIterator` (`next_back`,
`nth_back`, `rfind`, `rfold`), which the adapters implement where rustc's do;
`ExactSizeIterator` (`len`, in the prelude), for the slice, array and `Option`
iterators, the ranges of rustc's types and the adapters that keep it;
`IntoIterator`, which every iterator implements; `FromIterator`,
which `collect` builds through (for the program's types, `()`, pairs
of `Default + Extend` collections, and `Option` and `Result` of a
collection, which stop at the first `None` or `Err`), and `Extend`
(`unzip` and `partition` build with it); `Sum` and `Product` for the
integers, references to them, and `Option`s and `Result`s of them; and the iterators
`empty`, `once`, `once_with`, `repeat`, `repeat_with`, `repeat_n` (the
value's clones, then the value itself), `successors` and `from_fn`. Every adapter's
`size_hint` is rustc's, and the consumers and adapters that fold in
rustc's `core` (`count`, `last`, `for_each`, `sum`, `max`, ... and
`map`'s, `filter`'s, `chain`'s, ... `fold`s) fold here too, so their
closures run as often as in rustc. rustc specializes `Zip` and `eq`
for its `TrustedLen` and `TrustedRandomAccess` iterators (ranges,
slice, array, `str::bytes` and chunk iterators, flattened arrays, the
sources above, and adapters of them), pulling no item the other side
cannot pair; flux's `core` marks the same iterators with hidden
associated constants of `Iterator` (and arrays with one of
`IntoIterator`), which its `Zip` and `eq` test, so they behave alike;
marked `#[rustc_specialization]`, they leave `Iterator` dyn
compatible.
`&mut I` is an iterator where `I` is. The ranges of
integers are iterators (`Range`, `RangeFrom` and `RangeInclusive`, the
first and last from both ends). `try_fold`, `try_for_each` and
`DoubleEndedIterator::try_rfold` take a closure that returns an
`Option`, a `Result` or a `ControlFlow` (`core::ops`, with `is_break`,
`is_continue`, `break_value`, `continue_value`, `map_break` and
`map_continue`), and stop at the first `None`, `Err` or `Break`, which
they return. `for pat in head { body }` is
rustc's `match IntoIterator::into_iter(head) { mut iter => loop {
match Iterator::next(&mut iter) { None => break, Some(pat) => body }
} }`, so values are dropped as in rustc: the item's bindings at the end
of each iteration, the iterator when the loop ends, and the head's
temporaries after it. Its pattern must be irrefutable (E0005), its
body `()`, and `break` in it has no value (E0571).

A closure's parameter and return types come from its annotations,
from the `Fn`-family bounds of the parameter it is passed to (which
also fix which of the traits it implements), or from its body and
calls; it is inferred with the body around it, as in rustc. It
captures what its body uses of the bindings around it as rustc's
edition-2021 rules say: each place it uses (`x`, `s.a`, `*r`) by
shared reference, by mutable reference if it changes it, or by value
if it moves it, all by value with `move` (cut at the first
dereference); a place used whole and in part is captured whole. It is
`FnOnce` if it moves what it captures out, `FnMut` if it changes it,
and `Fn` otherwise, and `Copy` if what it captures is. The borrow
checker treats a closure as holding the borrows of what it captures,
for as long as it, or any value it is moved into (an adapter like
`Map`, a struct, a call's result), may be used, with rustc's messages.
A closure implements other traits through blanket `impl` blocks, as
any type does (`impl<F: FnMut(char) -> bool> Pattern for F`).
Safe function items and pointers implement the `Fn`-family traits too,
so they may be passed where closures are, and a closure that captures
nothing coerces to a function pointer of its signature (one that
captures is E0308, as in rustc).
Closures are returned as `impl Fn` (below); they cannot be `async`,
nor appear in constants or `const fn`s; calls of
`Fn::call` and the other methods by name, and `impl Fn for S`, are
unstable in rustc and not supported. A closure may return or keep
references to what it captures or is given. Passed to a generic
function, it gets the lifetimes the callee's `Fn`-family bounds say
its parameters and result have, as rustc's closure signatures do:
`Self::Item` of `map`'s `F: FnMut(Self::Item) -> B` is the iterator's
items, which it may return or keep, while a reference the bound
writes (`&Self::Item` of `filter`) is any lifetime at each call,
which nothing else may outlive (E0521, "lifetime may not live long
enough"). As in rustc, one whose parameters' written types have
references, and whose signature no bound gives, may not let them
outlive the call ("lifetime may not live long enough").

`impl Trait` in a function's parameter type (`f: impl Fn(i32) ->
i32`, `it: impl Iterator<Item = impl Copy>`) is an anonymous type
parameter with those bounds, after the function's others, as in rustc:
generic arguments give only the others (E0107), and `use<..>` is not
allowed there. In the return type of a free function or an inherent
method (`-> impl Iterator<Item = u64>`, `-> impl Fn(i32) -> i32 + '_`,
`-> Option<impl Fn()>`), it is an opaque type: callers see only its
bounds (its methods, calls of it, `for` loops over it, and `Copy` or
`Clone` only if a bound says so), while its body decides the type it
hides, which must meet them (E0277, E0271), be the same for every
return (E0308), and not contain itself (E0720); a body that only calls
itself hides `!`. A closure returned so gets its signature from the
`Fn` bound. As in edition 2024, an opaque type captures every generic
parameter and lifetime in scope (elided ones included), or those
`use<'a, T>` lists (which must name every type parameter): where it is
used, what they borrow stays borrowed, and its hidden type may borrow
only them (E0700, E0597, E0373, as in rustc), and only for as long as
its `+ 'a` bounds and what its bounds say associated types are
(`Item = &'a T`, elided lifetimes as in the return type) say. Auto
traits do not leak through it (flux has none besides `Sized`).
`impl Trait` in trait methods' return types, nested in an opaque
type's bounds or in bounds' generic arguments (`T: Iterator<Item = impl
Copy>`) is not supported yet; elsewhere it is rustc's E0562 ("`impl
Trait` is not allowed in field types", ...), or E0658 in type aliases
and associated types.

References may be anywhere a value may: parameters, return values
(`fn first(&self) -> &T`, `Option<&T>`), fields of structs and enums
(`struct Parser<'a> { src: &'a [u8] }`), tuples, other references,
`mut` bindings, `break` values, and constants and statics (`const P:
&[i32] = &[2, 3];`, shared ones only). Lifetimes are written as in
Rust: parameters on functions, structs, enums and `impl` blocks with
their bounds (`<'a, 'b: 'a, T: 'a>`, `where 'a: 'b`), `&'a T`, `&'a
self`, `'static`, `'_` and lifetime arguments (`Parser<'a>`), with rustc's elision rules for function signatures
(E0106 with rustc's help where they find nothing), `impl` headers,
constants and statics, and its errors for undeclared, outer, reserved
and unused lifetimes (E0261, E0401, E0262, E0637, E0392) and wrong
numbers of lifetime arguments (E0107, E0747). The bounds of functions
and `impl` blocks may name lifetimes in what they say associated types
are (`I: Iterator<Item = &'a T>`). Traits may not have lifetime
parameters yet, bounds may not name lifetimes in their trait's generic
arguments (`T: Tr<&'a U>`; `&` without one is E0637, as in rustc), nor
those of structs and traits in what they say associated types are.
Local `let` annotations can require `'static`, including nested reference
and ADT types, aliases and defaults. The borrow checker enforces this on
initialization, destructuring, later assignments and branches. Other named
body lifetime annotations (`let x: &'a i32`), lifetimes in function-pointer types and
lifetime-qualified associated projections are not supported yet. Nor are
lifetime annotations on uncaptured places in closures.
Direct function and method calls accept named, impl-owned and static lifetimes
inside written type arguments, such as `nth_child::<Expr<'t>>(node)`.
The borrow checker enforces these written regions and their well-formedness
constraints, including when the return value is unused. Named arguments in
closures and function-item values remain unsupported; constructor paths and
general named local annotations are outside this extension.
A trait's `impl` block may be for
some lifetimes only:
naming one twice (`impl<'a> PartialEq for Token<'a>`, whose `Rhs` is
`Token<'a>` too), naming `'static` (`impl Tr for &'static S`), or
bounding them (`impl<'a, 'b: 'a>`); a lifetime only its associated
types name is E0207, as in rustc.

Supertrait obligations are also checked at impl declarations, even for empty
or unused impls, under the lifetimes and bounds its header permits. A selected
supertrait impl may require only outlives relations that follow from that header.
Associated-type equalities on Self supertrait edges are proved against the
original header assumptions, comparing both types and represented lifetime
slots. Selected impl bounds also require matching projection types when no
lifetime slots are present. Nested selected projections that cannot be proved
from the header remain explicitly unsupported.
Associated-type definitions also check their declared bounds, including
associated-type equalities, under the impl header's universal lifetimes.
Projection selection on a definition's right-hand side must satisfy those
lifetimes even when its normalized result has no reference slots. Missing
projection input mappings remain explicitly unsupported.
Unrepresented supertrait argument regions and higher-ranked callback obligations
needed by a selected supertrait impl are explicitly unsupported; unrelated
callback bounds remain allowed.

`Fn`, `FnMut` and `FnOnce` bounds may return references whose lifetime
is supplied by the callable contract. Owner-bearing function items passed to
contracts introducing higher-ranked reference inputs remain explicitly
unsupported; this conservative boundary can reject valid methods with
independent late input lifetimes. Fixed type-parameter or iterator-item inputs
do not introduce such binders.
The same boundary applies to free function items with explicit signature
lifetime/type-outlives bounds or an own lifetime mentioned in a trait predicate.
Occurrences in associated equalities and alias arguments count even when type
normalization erases them. This includes tautological bounds; unrelated bounds
can also conservatively reject an otherwise valid late input. Missing callback
provenance is explicitly unsupported.
Unbounded named and elided free-function lifetimes remain supported.

Callable reference output lifetimes can be elided from a single input reference
(`F: Fn(&T) -> &U`). Each call's result borrows from its argument. This also applies to returned
opaque closures (`-> impl Fn(&u8) -> &u8`): the hidden closure must work
for every input lifetime, so a captured reference returned in this
example must be `'static`. A single-lifetime binder
expressing the same relationship (`for<'a> Fn(&'a T) -> &'a U`, also
in a `where for<'a>` clause) is supported. Other named lifetimes in
`Fn` bounds, binders with several lifetimes, and general higher-ranked
bounds are not supported yet.

The borrow checker infers lifetimes as rustc's (non-lexical lifetimes,
with two-phase borrows for method receivers and borrows of distinct
fields), with its messages: a reference returned or stored must not
outlive what it borrows (E0597, E0515 for what a function returns,
E0716 for temporaries), a function's lifetimes must relate as its
signature and bounds say ("lifetime may not live long enough"), and a
trait's `impl` block's method may not ask more of its lifetimes than
the trait's ("`impl` item signature doesn't match `trait` item
signature"). As in rustc, a trait's `impl` block is chosen by types
alone, and then its header's lifetimes and bounds must hold for the
call's: `x == y` of an invariant type needs the same lifetime on both
sides, and `impl Tr for &'static S` a `'static` reference (E0597 for a
borrow of a local, E0521, "borrowed data escapes outside of function",
for a parameter's). So must what a callee's bounds say its associated
types are, and a type that is `Copy` for some lifetimes only where a
value of it is copied. A borrowed constant expression (`&5`, `&[1, 2]`) lives
forever, as rustc promotes it.

Raw pointers and `core::ptr::read_unaligned` provide unsafe unaligned reads
of u8/u16/u32/u64/usize. They use alignment-one loads without creating source
references. Other type substitutions and const calls remain explicitly
unsupported; callers must provide initialized, readable memory.

Slices provide unsafe `get_unchecked` and `get_unchecked_mut` for `usize`
indices. The caller must supply an in-bounds index; unchecked range indexing
remains unsupported.

On AArch64, `core::arch::aarch64` provides distinct `uint8x16_t`,
`uint16x8_t`, `uint64x2_t`, `uint8x8_t` and `uint64x1_t` types, with their
Rust sizes/alignments and Copy/Clone/Debug. The bounded API includes
`vld1q_u8`, `vdupq_n_u8`, `vceqq_u8`, `vandq_u8`, `vorrq_u8`,
`vreinterpretq_u16_u8`, `vreinterpretq_u64_u8`, `vreinterpret_u64_u8`,
`vgetq_lane_u64::<LANE>`, `vget_lane_u64::<LANE>`,
`vshrn_n_u16::<N>` and `vpmaxq_u8`. Calls require an unsafe context;
const lane indices and narrowing shifts (`1..=8`) are checked after substitution. Ordinary
wrappers support unsafe function pointers; closures calling them inside an
unsafe block can satisfy Fn bounds. Safe pointer conversions and direct
Fn-bound use of the target-feature items are rejected. On AArch64, explicit
unsafe functions with bodies, including const functions and methods, may carry
`#[target_feature(enable = "neon")]`. Safe annotated functions, other features,
foreign declarations, required trait methods and `inline(always)` combinations
remain rejected. The attribute does not relax ordinary unsafe call rules.
Vector C ABI support remains unsupported. Narrowing currently lowers to scalar loads/shifts
and exactly eight byte stores. Pairwise maximum uses scalarized shuffles and
packed unsigned comparison/selection; native `shrn`/`umaxp` selection is not
implemented in the pinned Keel IR.

The two lane-extraction wrappers and narrowing shift also accept rustc's
legacy trailing const argument in direct path calls, including aliases and
macro expansions. Literals and resolved constant paths are supported; compound
legacy const expressions are explicitly rejected. The trailing argument is
never evaluated at runtime. Explicit turbofish calls, parenthesized callees
and function values retain the modern signature and its safety rules.

Unions support signed/unsigned integers, bool, unit, function pointers, fixed
arrays, concrete Copy structs, enums and nested unions whose stored fields
recursively contain only those types, with default/`repr(C)`
layout, Copy/Clone and whole-value moves. Fields share storage at offset
zero; size is the largest field size rounded to maximum alignment, and no
field niche is reused. Field reads/references require unsafe code, writes
require an initialized mutable union, and borrows of different fields overlap.
Copy/Clone copy storage without reading inactive fields or calling field Clone
implementations; fields have no implicit destructors. Struct type/const
arguments must be fully resolved, and Copy is checked through the trait solver.
Generic union declarations, nonempty tuples, stored references/raw pointers,
patterns, constant union storage, other derives/representation hints and field reinitialization after
a move remain explicitly unsupported. Whole-value assignment can initialize
or reinitialize a union; type-only size/alignment queries are supported.

Values are laid out in memory as rustc lays them out, so
`core::mem::{size_of, align_of, size_of_val, align_of_val}` (in the
prelude, as in rustc; `const fn`s, for sized types) give rustc's sizes:
struct and tuple fields reordered to leave little padding (in
declaration order with `#[repr(C)]`). `#[repr(align(N))]` on structs and
enums accepts unsuffixed power-of-two alignments through 2^29, preserving
aggregate size/stride and alignment in stack, static, argument/return and
heap storage; packed, transparent and SIMD representations remain unsupported.
The layout also keeps enums' tags as small as their
discriminants allow, and niches (`Option<&T>`, `Option<bool>`,
`Option<NonNull<T>>` take no more room than what they hold). `core::ptr`
has `null`, `null_mut`, `eq`, `read`, `write`, `copy`,
`copy_nonoverlapping`, `write_bytes`, `swap`, `replace`,
`drop_in_place` and `NonNull` (`new`, `new_unchecked`, `dangling`,
`as_ptr`, `as_ref`, `as_mut`, `cast`, `add`, `read`, `write`), and raw
pointers have `is_null`, `cast`, `cast_mut`/`cast_const`, `addr`,
`add`, `sub`, `offset`, `wrapping_add`, `wrapping_sub`, `offset_from`,
`read`, `write`, `replace`, `swap`, `drop_in_place`, `write_bytes`,
`copy_to`, `copy_from` (and their `_nonoverlapping` forms), `as_ref`
and `as_mut`; a `*mut T` coerces to a `*const T`. `core::mem` has
`forget`, `ManuallyDrop` (with `Deref`) and `MaybeUninit` (`new`,
`uninit`, `zeroed`, `write`, `as_ptr`, `as_mut_ptr`, `assume_init` and
its `_read`, `_ref`, `_mut` and `_drop` forms). Pointer arithmetic and
casts of integers to pointers are not supported in constants yet.

At run time flux matches rustc with `-C overflow-checks=off -C
panic=abort`: arithmetic wraps, shift amounts are masked to the type's
width, and division by zero or `MIN / -1` panics. Programs print with
`println!` and its relatives and call C through `unsafe extern "C"`
blocks. The embedded `std` facade allows paths such as `std::fmt`,
`std::mem`, `std::ops::Index` and `std::vec::Vec` to name the original
`core`/`alloc` definitions. Its only thread API is `thread::panicking()`,
which returns false because Flux aborts without unwinding. It provides no
thread creation, filesystem, atomics or synchronization.

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
  `extern` blocks. The parser sees only token kinds, so these need the
  lexer to mark candidate identifiers first (`&raw const` needs none: only
  there may `const` or `mut` follow an identifier).
- Negative impls (`impl !Send for T`), `box` patterns, `yield` and `gen`
  blocks, `try` blocks.
- Shebang lines and frontmatter.
- Validation that rustc does after parsing: escape sequences in literals
  outside bodies (lowering checks those in bodies and constants),
  suffixes, attribute placement, `let` outside conditions.
