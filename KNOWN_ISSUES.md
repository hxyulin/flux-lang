# Known issues

Found by a read-only code review of `eb59ac9` (2026-10-29), with keel read
at the then-pinned revision `f37f0bd`. Line numbers refer to `eb59ac9`.
Issues fixed since then are listed under [Fixed](#fixed) and removed from
the sections below.

Each entry has a confidence tag:

- **observed**: seen by running flux and rustc while fixing other issues.
- **re-checked**: the mechanism was confirmed by a second read of the source.
- **traced**: a reviewer followed the code path end to end by reading.
- **plausible**: likely, but part of the path was not traced.

Unless an entry is tagged observed, claims about rustc's behavior come from
the reviewers' knowledge of rustc, not from running it.

On 2026-11-04 every entry in the four correctness sections was re-run at
`9eabfc31` through rustc 1.99.1 (`--edition=2024`) and `fluxc run`. Entries
that reproduced are now tagged observed, and their locations refer to
`9eabfc31`. The same survey ran 13,000 difftest programs and about 45
hand-written ones; the bugs it found are under
[Found by differential testing](#found-by-differential-testing). In none of
them did flux and flux `-O` disagree, so none points at keel. The repro
programs and both compilers' outputs are in `out/survey-2026-11-04/`
(`correctness-known/r/` and `correctness-new/repros/`), which is not
committed.

## Fixed

These are on `main`, in `43909dd..a62ae8e` (keel `d17902e`). Each fix has a regression fixture or `tests/run` program
that failed before it. The full `just check` sequence (format, workspace
clippy, script tests, the fast and complete suites, and workspace
`flux check`) passed on the branch, except the three self-host targets,
which passed after `a62ae8e`.

- Parser panics on `X<Y<Z + W>: V>`, `S { #[a] { : 0 } }` and `[:]`
  patterns: `44ef56d`, `7e307f2`, `fa497cf`. List loops now skip an element
  that consumes no token.
- Parser accepted `a == b == c`, `a..b..c`, fields after `..base`, and
  `const`/`async`/`extern` before `impl` or `trait`: `8e3bea6`, `7e307f2`,
  `8bf4bba`.
- "expected BANG", "expected FN_KW" and "expected PIPE" messages: `bd7ee6b`.
- `Tree::parent()` was O(depth × siblings) per call: `f53ad9b`. `flux hir`
  on 32,000 `assert_eq!` statements went from 0.86 s to 0.53 s.
- `cfg(true)` and `cfg(false)`: `1580d47`.
- String line continuations in CRLF files: `dc10938`.
- Leading byte order mark: `2b46703`, `a62ae8e`.
- Stale interner and `union` docs: `332cefe`.
- False E0207 for parameters used only in a trait object: `859fe0e`.
- Missed E0038 for `where Self: Tr<Self>` supertraits: `9a1f2e3`.
- Overlap check did not unify function-pointer types: `3118e06`.
- Impl index had no float head: `6086470` (not measured).
- Discriminant reads were not checked against loans (E0503): `93f4c46`.
- `call_regions` went stale after drop elaboration renumbered blocks; the
  E0597 program from the review reproduced: `3cfd2d5`.
- Shallow writes only skipped a dereference in the next projection:
  `3dfa2ca`.
- Overflow lint flagged parameters assigned once: `334b844`.
- Overlapping array moves through slice patterns (double drop): `a9adedc`.
- `match` on `repr(i128)`/`repr(u128)` enums produced invalid keel IR, and
  case values were truncated to 64 bits: `6fb5502`.
- Shift-amount and i8/i16-to-float widening workarounds deleted (K6, K12):
  `3e92165`, `fd52c50`.
- Same-size int/float transmutes now use keel `bitcast`: `fa95539`.
- A 16-byte widened enum tag kept stack garbage in its upper half:
  `ea6b7f9`.
- `let mut x = None; x = Some(x);` overflowed the stack: `6d92eea`.
- `println!("{}", 1.5)` failed with "type annotations needed": `39376db`.
- Inherent impl selection ignored repeated parameters (`Pair<T, T>`):
  `86fdeab`.
- `if`/`match` discarded a failed `unify` after `can_unify`: `221ef05`.
- Safe and unsafe fn pointers did not join in branches: `dc4c65e`.
- Casts to fn-pointer types were lowered as integer truncations, which keel
  rejected: `3706a32`.

Found fixed when re-checked at `9eabfc31`; the fixing commits were not
identified:

- `use m::*;` in the glob-order example below gave a false "`x` is
  ambiguous". It now prints 2, as rustc does.
- Syntax-error columns were one less than rustc's.
- Deep nesting no longer crashes the parser: `fluxc check` accepts 100,000
  nested parentheses, prefix `-`, `&` types and attribute parentheses, and
  50,000 `else if` and `Option<` levels. rustc crashes with SIGBUS on all
  six.

## Correctness: miscompiles and invalid Rust accepted

### Glob names used before the glob's source module is final

observed. `crates/flux-hir/src/def_map.rs:1081-1170`; `apply_glob` at
`1457`.

```rust
use m::x;
mod inner { pub fn x() -> i32 { 1 } }
mod deep { pub fn x() -> i32 { 2 } }
mod m { pub use crate::inner::*; pub use crate::other::x; }
mod other { pub use crate::deep::x; }
fn main() { println!("{}", x()); }
```

rustc prints 2; flux prints 1. Round 1 applies `m`'s glob, so `m.x` is
`inner::x`. In round 2, the root's `use m::x` binds to that glob entry and
is never resolved again; `m`'s explicit import then overwrites `m.x`.

Fix: follow rustc's determinacy rule (a module's glob names are not usable
while it has unresolved single imports that could shadow them), or
re-resolve dependents when a source binding changes. Effort: half a day to a
day.

### Macro fragments other than `expr` are pasted as raw tokens

observed. `crates/flux-hir/src/macros/mbe.rs:1425-1426`.

```rust
macro_rules! a { ($l:literal) => { $l.abs() } }
fn main() { println!("{}", a!(-1i32)); }
```

rustc treats `$l` as one expression and prints 1. flux re-parses
`- 1i32 . abs()` as `-(1i32.abs())` and prints -1. By the same mechanism,
`&$t` with `$t:ty = dyn Debug + Send` is rejected with 8 parse errors, and
`$t::default()` is accepted and prints 0 (rustc: "missing angle brackets in
associated item path"). Parenthesized `pat` captures already work.

Fix: add `FragKind::Literal` to the wrap condition (under 2 hours). Wrapping
`ty` captures that contain a top-level `+` fixes the `dyn` case. The
`$t::default()` case needs real invisible delimiters for every capture
except `ident`, `tt` and `lifetime`.

### Coherence accepts overlapping impls

observed. `excluded_by_bounds` in `crates/flux-ty/src/traits.rs:1421-1446`.
Both examples below are accepted; calling a method of the first pair fails
with "type annotations needed", and the second prints 2.

`excluded_by_bounds` treats `!holds(...)` as proof that two impls cannot
overlap. It treats the other impl's parameters as fixed types, and it
counts bounds on foreign traits and types as negative evidence, although an
upstream crate may add such an impl.

```rust
trait Tr {}
impl<T: Copy> Tr for T {}
impl<T: Clone> Tr for T {}          // rustc: E0119

impl<T: core::fmt::Display> Tr2 for T {}
impl Tr2 for Vec<u8> {}             // rustc: E0119
```

Downstream, `select_impl` finds two candidates and returns `None`, and
`resolve_instance` (`traits.rs:2547-2563`) silently returns the trait's own
method (plausible miscompile). Fix: count a failing bound as negative
evidence only when it has no parameters left and is knowable (local trait
or local input type). Effort: half a day to a day; `library/core` may rely
on the current behavior.

### `?Sized` parameters count as sized outside bodies

observed: all three examples below are accepted.
`crates/flux-solve/src/traits.rs:1625-1627` calls `maybe_unsized(.., &[], ..)`,
so every parameter counts as sized. The E0276 check is at
`crates/flux-ty/src/traits.rs:1348-1355` and the Drop check at `3448-3456`.

`holds_at` answers every `Sized` goal on a parameter with "yes", and
`match_impl_sized` uses `sigs.is_unsized` ("definitely unsized") instead of
the item's `unsized_params`.

- `impl Tr for u8 { fn f<T: ?Sized>(_: &T) -> usize where T: Sized { ... } }`
  for a trait method `fn f<T: ?Sized>(x: &T)` is accepted. rustc reports
  E0276. `<u8 as Tr>::f::<str>` can then reach monomorphization.
- `impl<T> Tr for Box<T> {} struct Need<X: Tr>(X); fn g<U: ?Sized>(_: &Need<Box<U>>) {}`
  is accepted. rustc reports E0277.
- `check_drop_impls` skips every `Sized` bound, so
  `struct S<T: ?Sized>(Box<T>); impl<T> Drop for S<T>` misses E0367.

Fix: pass `unsized_params` into `holds` and `match_impl`, as
`SignatureImplProof` already does. That touches about 36 `holds` call sites
(more than a day). The E0276 and E0367 cases have targeted fixes of under 2
hours each.

### Re-exports do not cap visibility

observed: both programs below print "f". The `wider` check is at
`crates/flux-hir/src/def_map.rs:1239-1244`, the glob binding's visibility at
`1519`.

```rust
mod outer {
    pub mod a { pub(super) fn f() {} }
    pub mod b { pub use super::a::*; }
}
fn main() { outer::b::f(); } // rustc: E0603

mod m { fn f() {} pub(super) use self::f as g; }
fn main2() { m::g(); }       // rustc: E0364
```

The glob binding takes the glob's `pub`, and `wider` only flags `pub` and
`pub(crate)` imports; it has no case for two module-scoped visibilities.
Across crates this leaks `pub(crate)` items. Fix: give the binding the
narrower of the two visibilities, reusing the narrowing that lines 1474-1484
already do for anonymous traits.

### Ambiguous glob names fall through to the prelude

observed for patterns. `lookup` (`crates/flux-hir/src/def_map.rs:701`)
skips ambiguous globs; the pattern path is `lower.rs:5330-5334`. In
expressions flux already reports "`X` is ambiguous" (`def_map.rs:351`,
`438`), and the prelude case is also correct.

```rust
mod a { pub const X: i32 = 1; }
mod b { pub const X: i32 = 2; }
use a::*; use b::*;
fn main() { match 2 { X => {} } } // rustc: E0659
```

flux binds a fresh `X`. Fix: report "`X` is ambiguous" the way the
expression path does (under 2 hours).

### Drop order and temporary scopes

observed.

- `let ref _x = D("a");` drops the temporary at the end of the statement.
  rustc extends it to the block. When the binding is used afterwards, flux
  rejects the valid program with "temporary value dropped while borrowed".
  `extending_refs` only collects `&` expressions
  (`crates/flux-mir/src/build.rs:4310-4330`, `lower_let` at `4533`). Fix:
  when the pattern has a `ref` binding, allocate the initializer's temporary
  at `extend_at` (half a day).
- A failed `if let` match guard never drops its scrutinee's temporary. With
  `fn mk() -> Result<u8, D> { Err(D("g")) }`,
  `match 0 { _ if let Ok(_) = mk() => {}, _ => println!("other") }` prints
  only "other"; rustc prints "drop g" first. `lower_condition` runs with no
  temporaries scope (`build.rs:5394`), and the failed edge (`goto
  exit.resume`, about line 5422) emits no drops. Fix: drop the guard's
  temporaries on the failed edge only; when the guard passes, rustc keeps
  them through the arm body (under a day).

### `#[track_caller]` location lost when the function's address is taken

observed: rustc reports the panic at 5:5, flux at 2:36 inside the callee.
`crates/flux-codegen/src/lower.rs:1298-1309`, with emission at `492` and
`511-513`.

Reified instances get no location parameter, and that also applies to
direct calls. With `let f: fn(Option<u32>) -> u32 = must; must(None);`, the
panic reports a line inside `must`. Separate compilation already emits a
`.__flux_reify` twin that passes a fixed location; whole-program builds
(what `fluxc run` does) drop the location parameter from every call
instead. Fix: emit the twin whenever the instance is reified (under 2
hours).

### Closure captures and packed structs

observed. `crates/flux-ty/src/infer/closure.rs:904`.

`truncate_capture` lacks rustc's rule that a by-reference capture stops
before any field of a `repr(packed)` struct. `let c = || p.b; p.a = 5; c();`
with a `u8` field is accepted and prints 2 (rustc: E0506). With a `u32`
field, flux-mir's alignment check rejects the valid capture with "reference
to packed field is unaligned". Fix: stop before the first field of a packed
struct whatever its alignment (under 2 hours).

### Closure region requirements can be dropped

plausible; no failing program was found on 2026-11-04 either. The code is
unchanged at `crates/flux-mir/src/borrowck/regions.rs:7565-7586` and
`7844-7846`.

When `bound_slots_in` returns `None`, `Upvar(k)` becomes `ClosureSlot::None`
and requirements such as `(Arg(0), Upvar(0))` hit `_ => {}`. The closure is
already in `proven_closures`, so `passed_closure` skips every `(Arg, _)`
requirement. No failing program was constructed. Fix: report "not
represented" when either endpoint is `None`, and drop the
`signature_known` skip.

### Borrowck fallbacks that add no constraints

traced (mechanism), latent. At `9eabfc31`, `sinks` is at
`crates/flux-mir/src/borrowck/regions.rs:7908-7963` and still ends in
`_ => {}` without advancing the slot index. The line numbers below are from
`eb59ac9`.

- `place_regions` returns empty slots for an unknown projection
  (`borrowck/regions.rs:4280-4283`); `relate` then does nothing
  (`3932-3940`), and `Rvalue::Ref` adds no loan constraint (`4889`).
- `Rvalue::Cast` to a reference relates nothing unless it is a
  reference-to-raw-pointer cast (`4456-4471`). Currently unreachable.
- `sinks` and `closures_in` (`7747-7786`, `7572-7607`) have no arm for
  `Dynamic`, `Alias` or `FnPtr` and do not advance the slot index, so
  `opaque_call` attaches constraints to the wrong variables.

Fix: emit a bug or "not supported" diagnostic in each fallback, and add a
debug assertion that `relate` never gets exactly one empty side.

### Other accept-invalid cases

All observed at `9eabfc31`.

- **Casts of literals.** Only an unsuffixed integer literal takes an `as`
  target's type (`crates/flux-ty/src/infer.rs:3207-3222`). As a result:
  - `1.00000005960464477550 as f32` is typed as an `f64` literal and then
    cast, which double-rounds: flux prints `1`, rustc `1.0000001`. This is a
    miscompile. `-1.00000005960464477550 as f32` gives the bits 3212836864
    where rustc gives 3212836865. Annotated literals, call arguments,
    `vec!` and arrays are correct.
  - `1e40 as f32` prints `inf` (rustc: deny lint).
  - `-1 as u32` prints 4294967295 (rustc: E0600), and `-200 as i8` prints
    56 (rustc: deny lint).

  Fix: look through unary `-` and `!` to the literal; give an unsuffixed
  float the float target's type, an unsuffixed integer the integer
  target's type, and an integer `usize` when the target is a raw pointer.
  The existing range and E0600 checks then fire. This also fixes the
  rejected hex-to-pointer casts under "Valid Rust rejected". About 15
  lines.
- The overflow lint never looks through fields or aggregates:
  `let t = (255u8, 1u8); let _ = t.0 + t.1;` is accepted and prints 0
  (`crates/flux-mir/src/lint.rs:201-208`, `347`; a projected write skips
  invalidation at `68`).
- The packed-field alignment check ignores enum discriminants given by
  constants, so `enum E { A = D, B }` with `const D: isize = 1000` inside a
  `#[repr(packed)]` struct lets `&p.e` through (rustc: E0793).
  `crates/flux-mir/src/alignment.rs:58-62` builds layouts with default
  discriminants.
- Const eval allows 50 million steps counted per statement
  (`crates/flux-mir/src/eval.rs:182`). rustc's `long_running_const_eval`
  lint fires for loops of 1 and 2 million iterations but not 500,000; flux
  fires only at 5 million. Fix: count terminators and lower the limit.
- Normalization in signatures ignores rustc's preference for where-clauses
  over blanket impls, so a method using `u8` for `<T as Tr2>::Out` is
  accepted (rustc: E0053) (`crates/flux-solve/src/traits.rs:1029-1100`).
- A duplicate `#[macro_export]` macro silently replaces the first and the
  program prints 2 (rustc: E0428)
  (`crates/flux-hir/src/item_tree/collect.rs:622`, `273`).
- Raw strings with more than 255 `#` are accepted
  (`crates/flux-syntax/src/lexer.rs:377-397`).
- `impl<T> Tr for T where W<T>: Tr {}` on its own is accepted (rustc:
  E0275). See the recursion-limit entry below.
- `std::iter::Sum::sum::<I>(..)` is accepted (rustc: E0790, cannot call an
  associated function on a trait without naming the type).
- `<u8 as Arb>::arb() + Arb::arb()` is accepted (rustc: E0284, type
  annotations needed). The second call's `Self` is inferred from the `Add`
  impl instead of being left ambiguous. Found by the lifetimes agent on
  2026-11-05; not reduced further.

### Struct fields of type `<&mut dyn Trait as Trait>::Item` get the wrong layout

Found on 2026-11-05 while rerunning the corpus, and reproduced at
`546c8712`. `r.peekable().peek()` with `r: &mut dyn Iterator<Item = i32>`
prints `None` or `Some(0)`, or traps, where rustc prints `Some(1)`. Type
checking picks the right `Self`. The bug is in `field_types`
(`crates/flux-solve/src/struct_fields.rs`): `may_hold` rejected
`impl Iterator for &mut I` because `dyn Iterator` has no `impl` block, and
`normalize_known` could not read a trait object's own associated types. So
`Peekable::peeked: Option<I::Item>` kept an unresolved type. Every build
script that uses the `cc` crate crashed with SIGSEGV (psm, stacker and
others). Fixed by `ee03b753` and `a4c960bc` on `fix/unsized-iterator-methods`,
not yet on `main`.

## Correctness: crashes

### Unbounded recursion on deep nesting

observed after parsing; the parser no longer crashes (see Fixed).

- 5,000 and 7,000 nested parentheses compile and run. At 10,000, `fluxc
  hir`, `mir` and `run` abort with a stack overflow; rustc already crashes
  at 7,000.
- `fluxc parse` aborts at 100,000 nested parentheses, although `fluxc
  check` passes.
- A raised `#![recursion_limit]` lets macro expansion recurse natively
  through `lower_expr`, `expand` and `collect_body_node`, and the
  macro-recursion repro aborts (rustc: SIGBUS).

flux survives deeper nesting than rustc, so this is low priority. Fix: run
compilation on a thread with a large stack (256 MiB) in
`crates/flux-cli/src/driver.rs`, and keep a nesting limit for the tree
printer.

### Hygiene counters panic in long-lived processes

traced; not reproduced, because it needs 2^31 contexts. The code is
unchanged at `crates/flux-hir/src/macros/mod.rs:159-180`.

Hygiene contexts and token group numbers come from process-wide atomic
counters that never reset; the `assert!` at line 165 eventually panics in
difftest or a future LSP.

### Concurrent host preparations fail

observed on `perf/enum-memory` (2026-11-05), not reduced. Two `flux build`
runs that prepare native hosts at the same time can fail with "failed to
open object file" while archiving host libraries. `perf/driver` added a lock
file that serializes preparations of one workspace, but the host artifact
cache (`native-host/deps/`) may still be shared across processes without
one. On 2026-11-06 two corpus runs of 4 jobs each, in separate case
directories, failed this way in 6 of 853 crates ("failed to map object
file: memory map must have a non-zero length" for `libsyn`, `libquote` or
`libproc_macro2` rlibs); each built normally when rerun alone.

### Debug assertions fire on corpus crates

observed on 2026-11-06 at `9aaf0e96`, not reduced. With debug assertions
on (the `stage0` profile), 20 corpus crates that the release build reports
as diagnostics, and `array-init`, which it builds, panic instead. Most stop at
`debug_assert!(false, "types are lowered in a file")` in `anon_kind`
(`crates/flux-hir/src/item_tree.rs:3467`), reached from
`lower_const_arg` through `lower_path`: a const generic argument in a path
lowered outside a types file. Others: `ndarray` at
`crates/flux-ty/src/infer.rs:2424`, and `utf8_iter` and `utf_types` at
`crates/flux-mir/src/build.rs:160`. The release build falls back to
`ArrayLenKind::Error`; compare corpus runs between builds of one profile.

## Correctness: valid Rust rejected

All observed at `9eabfc31` unless noted.

- **`as_flattened` in constants.** `const F: &[u8] =
  [[1u8, 2], [3, 4]].as_flattened();` gives "reinterpreted raw slices in
  constants are not supported yet"; the evaluator cannot cast
  `*const [T; N]` to `*const T` and rebuild a slice. Calls outside
  constants work. Observed 2026-11-06 on `chore/keel-f503b726`.

- **References to generic associated constants are not promoted.** In a
  generic function, `RawWaker::new(p, &Callbacks::<W>::VTABLE)` gives
  "temporary value dropped while borrowed"; rustc promotes the constant to
  `'static`. A reference-typed constant
  (`const VTABLE: &'static T = &T::new(..)`) works. Observed on
  `feat/corpus-integration` on 2026-11-05, while writing `std::task::Wake`.
- **`#[allow]` and `#[expect]` are dropped**, so flux-mir's deny lints
  cannot be silenced: `#[allow(arithmetic_overflow)]` on a function with
  `255u8 + 1`, and `#[allow(unconditional_panic)]`, are rejected
  (`crates/flux-hir/src/item_tree/attributes.rs:347-350`; the lints run at
  `crates/flux-mir/src/build.rs:279`). Fix: record allowed lints per
  function, inherited from impl, module and crate attributes, and filter the
  lint results (half a day to a day).
- **Promotion skips arithmetic, comparisons, casts and `!`.**
  `&(1 + 2)`, `&(1 < 2)`, `&(300i32 as u8)` and `&!true` returned as
  `&'static` each get E0515. `crates/flux-mir/src/build.rs:2849-3086` has no
  arm for these operators.
- **Loans through raw-pointer derefs are tracked.**
  `let a = &mut (*p).0; (*p).0 = 5; *a += 1;` gets E0506; rustc ignores
  such borrows. `collect_loans` (`crates/flux-mir/src/borrowck.rs:219-250`)
  should skip a loan whose place goes through a raw-pointer deref, as
  rustc's `ignore_borrow` does. This may matter for self-hosting. Under 2
  hours.
- **Uninhabited variants in nested patterns.**
  `match (r, 1) { (Ok(v), _) => v }` with `r: Result<i32, Infallible>` gets
  E0004; sub-columns always get `at_scrutinee = false`
  (`crates/flux-ty/src/pats.rs:936`, `954`). Fix: keep `at_scrutinee` for
  field columns and clear it only under a reference or a union field.
- **Large hex literals cast to pointers.**
  `0x1_0000_0000 as *const u8` is rejected as out of range for `i32`, and
  so are casts to `*mut u8` and `*const ()`. Same cause and fix as the
  literal casts under "Other accept-invalid cases".
- **Recursion limits give wrong answers instead of E0275.** observed; a
  diagnostic difference only. Valid 40- and 100-level nesting compiles and
  runs. Fix: turn the depth cut-offs into an overflow result reported as
  E0275 (more than a day).
  `holds_at` stops at depth 256 (`traits.rs:2125`), `select_impl` at 32
  (`1788`), `SignatureImplProof` at 32 (`879`, `906`), `may_hold` at 8
  (`1869`), the normalizer at 64 (`1625`), each returning "does not hold" or
  "ambiguous". observed for `struct W<T>(T); impl<T> Tr for T where W<T>: Tr {}`:
  - Inside bodies, flux already gives rustc's E0275 text ("overflow
    evaluating the requirement `u8: Tr`").
  - In a signature, `fn g(_: Need<u8>)` gets an E0277 naming a type with 33
    nested `W`s.
  - In an impl where-clause, `where u8: Tr` gets an E0277.

  A chain of 40 `.map` adapters type-checks, so the 32-goal limit does not
  show in inference.
- **Repeated impl parameters in the duplicate-definition check.**
  `impl<T> Pair<T, T> { fn second(..) }` next to
  `impl Pair<u8, i32> { fn second(..) }` gives "duplicate definitions with
  name `second`"; rustc accepts it. This is the same flaw that `86fdeab`
  fixed in inherent method selection. `overlap` is at
  `crates/flux-ty-ir/src/lib.rs:308`; bind each side's parameters and compare
  later uses against the binding.
- **No `Sum` or `Product` impl for floats in flux's `core`.**
  `[0.5f64].iter().sum::<f64>()` fails to type-check. Add 8 impls next to
  the integer ones (`library/core/src/iter.rs`, about line 2488). An empty
  float `Sum` gives `-0.0` in rustc.
- **Inferred struct outlives facts stop after 16 rounds.** traced; could not
  reproduce on 2026-11-04. 20- and 40-level chains in reverse order and a
  cycle all compile. `crates/flux-ty/src/regions/resolve.rs:3212` still
  loops `0..16`.
- **Literal suffixes and reserved prefixes.** `"a"x` lexes as two tokens:
  a token-counting macro gives 2, rustc 1. `z"x"` is accepted (rustc
  rejects the reserved prefix). `crates/flux-syntax/src/lexer.rs`, around
  line 400.

## Diagnostics that differ from rustc

All observed against rustc 1.99.1, and re-checked at `9eabfc31`.

- A cyclic type (`x = Some(x)`, `v.push(v)`) reports a type mismatch. rustc
  reports E0275 ("overflow evaluating whether `Option<_>` is
  well-formed"). flux has no E0275.
- `if c { (a, a) } else { (1i32, 2u8) }`: both compilers now report the
  same message at the same column. Only the type text differs: rustc says
  `({integer}, {integer})`, flux says `(i32, i32)`.
- `1..2..3` reports "range operators cannot be chained", followed by a
  cascaded "mismatched types". rustc reports "expected one of `.`, `;`, `?`,
  `else`, or an operator, found `..`".
- `async impl` and `async trait`: the wording follows the project's
  convention, but flux reports column 6 (`impl`) and rustc column 1
  (`async`).
- Calling `h.wrapping_mul(..)` on an ambiguous `{integer}` reports "not
  supported yet". rustc reports E0689 ("can't call method on ambiguous
  numeric type").

## Found by differential testing

Found on 2026-11-04 at `9eabfc31`. All observed against rustc 1.99.1 with
the bootstrap flags (`-Cpanic=abort -Coverflow-checks=off
-Cdebug-assertions=yes`), with the same result from flux with and without
`-O`.

Difftest ran 13,000 programs in five campaigns: 3,000 with default weights
and the rest biased toward drops, closures, iterators, panics and wide
integers. 19 failed, from the first two causes below. About 45
hand-written programs found the rest. Separately, 50 programs that rustc
rejects were all rejected by flux.

### Miscompiles

- **Locals declared in an or-pattern arm's guard lose their scope.** In
  `match x { 0 | 1 if { let mut d = D(x); d.0 += 0; d.0 == 1 } => 10, _ => 20 }`
  inside a loop, rustc prints `drop 1` and flux never drops `d`. With an
  immutable `let`, or any `println!`/`format!` in such a guard (through
  their internal `args` binding), flux reports "cannot assign twice to
  immutable variable" on valid code. That accounts for 17 of the 19
  difftest failures. In `declare_bindings`
  (`crates/flux-mir/src/build.rs:4641-4645`), the early `continue` when
  `binding_locals` already has the binding also skips `scope.locals.push`
  at `4662-4668`, so the second copy gets no `StorageDead` and no drop. Fix:
  push the existing local onto the current scope before `continue` (about 5
  lines).
- **An early exit while evaluating operands drops them in the wrong
  order.**

  ```rust
  f(D("arg0"), D("recv-temp").m(), { if flag { return; } 3 });
  ```

  rustc drops `arg0`, then `recv-temp`; flux drops them in reverse. The
  same happens in tuples, tuple-struct constructors, arrays, method calls,
  and with `break` or `?` inside an argument. `temp_for`
  (`build.rs:1833-1843`) puts moved operands and borrowed temporaries in
  one `Temps` scope, ordered by creation; rustc schedules a moved operand's
  drop in the call's own inner scope. Fix: give call and aggregate
  operands an inner scope (30 to 60 lines, plus `.mir` snapshot changes).
- **`[x; 0]` never drops `x`.** `let z = [D("rep"); 0];` prints nothing;
  rustc prints `drop rep`. The same happens with a `const N: usize = 0`
  length. `build.rs:3528-3535` lowers it to `Rvalue::Repeat` with no drop.
  Fix: drop the operand when the count is 0 and its type needs drop (about
  10 lines).

### Valid Rust rejected

- Calling `Box<dyn Fn>` or `&Box<dyn Fn>` directly (`b(1)`) gives "expected
  function, found `Box<dyn Fn..>`", and `(**f)(x)` gives "size ... cannot
  be known". `infer_value_call` (`crates/flux-ty/src/infer/closure.rs:507-590`)
  does not autoderef through `Box` or autoref a `dyn Fn` place. The
  registry survey hit the same gap with `Box<dyn FnMut>` in 19 packages.
- `Err(e)?;` as a statement gives "type annotations needed". `infer_try`
  (`crates/flux-ty/src/infer.rs:3346-3392`) should mark the output variable
  as diverging, so the edition-2024 `!` fallback applies.
- A trait method returning `impl Iterator<Item = u8>`: `.collect::<Vec<u8>>()`
  on its result fails because `<impl Iterator as Iterator>::Item` is not
  normalized at the call site. docs/LANGUAGE.md says this is supported.
- `let o = Some(&D(..));` and `W(&D(..))` give "temporary value dropped while
  borrowed". `extending_refs` (`crates/flux-mir/src/build.rs:4474-4510`) has
  no arm for tuple-struct or variant constructor calls.
- `Box<dyn Iterator<Item = i32> + '_>` built from
  `once(self.v).chain(self.kids.iter().flat_map(|k| k.walk()))` gives
  "associated type ... may not live long enough": the region proof does
  not go through `FlatMap`.
- `Box::new(|| 1) as Box<dyn Fn() -> i32>` gives "non-primitive cast". The
  `as` check (`infer.rs:7543-7553`) lacks the unsizing coercion that a
  `let` annotation already accepts.
- `let r: Rc<dyn Debug> = Rc::new(5);` is rejected, and `Rc<T>` has no
  `Debug` impl in flux's `alloc`.

### Not reduced yet

A build of 849 registry crates (see the features survey in
`out/survey-2026-11-04/SURVEY.md`) produced diagnostics that are likely false
rejections but were not minimized: "type parameter is never used" in 13
packages, "conflicting implementations" in 11, and 24 "cannot borrow `self`
as mutable" errors in `bytes`.

## Keel workarounds and keel issues

Written against keel `f37f0bd`. "Keel source" paths are under
`~/.cargo/git/checkouts/keel-6e3ae8ed5fc61c1e/f37f0bd/crates/`. On
2026-11-05 every open item was re-read in keel's source at the pin
(`a078986`) and at local keel `main` (`62584dcb`), without building. All
are still open at both, except as noted on K17, K18 and K20. The aarch64
alignment bug got wider: the verifier now allows slots aligned to 64.

On 2026-11-06 the pin moved to `f503b726` on `chore/keel-f503b726`. At
that commit keel implements K2 (multiple results), K8 (CAS with success and
failure orderings, a success flag and a weak form), K14 (fast direct calls
to address-taken functions), K18 (slot lifetimes and coloring), K19
(128-bit atomics) and K21 (atom-by-atom dead strip), and the error types
implement `std::error::Error`. flux uses K14, which needs nothing from it,
and K21: Mach-O links now set `dead_strip`, which cuts `fluxc` from 66 MB
to 54 MB and the `flux` driver from 8.1 MB to 2.1 MB. flux does not use
K2, K8, K18 or K19 yet; its CAS lowering passes `cas_failure()` of the one
ordering and keeps only the old value. The table rows below are not
rewritten for this.

### Keel slowdown at `f503b726`

Measured 2026-11-06 on arm64 macOS by building `fluxc` from the same
source (`main` at `30c0285e`) with flux built by rustc at both pins:

| Compiler profile | `a078986` optimize | `f503b726` optimize | Whole build |
|---|---|---|---|
| release | 20.7 s | 26.5 s (+28%) | 56 s to 63 s |
| stage0 (debug assertions) | 36.8 s | 86.6 s (+135%) | 75 s to 126 s |

The debug-assertion share of keel-opt grew from 16 s to 60 s. The
flux-built stage-2 build got faster (264 s to 199 s), so keel's output
improved while its own optimizer got slower.

### Keel bugs

- **aarch64 stack slots aligned above 16.** The verifier allows 32
  (`keel-ir/src/verifier.rs:704`) and x86 realigns
  (`x86_64.rs:771-778`), but aarch64 assumes at most 16
  (`aarch64.rs:2112-2115`). flux pads by hand (`lower.rs:1872-1895`), which
  costs align-1 bytes and 5 instructions per slot and hides the slot from
  stack-forward and stack-promote.
- **Multi-return C signatures.** The verifier accepts them
  (`verifier.rs:1956`), and aarch64 codegen rejects them
  (`aarch64.rs:7337-7340`).
- **Error types.** `OptError` and the linker errors do not implement
  `std::error::Error` (flux `crates/flux-codegen/src/error.rs:88`).
- **`weak_odr` addresses on Mach-O.** observed by the `.flib` codegen work
  when linking two objects, and re-checked in keel's source at `d17902eb`
  and keel `main` `0862d28d`. Reported to the keel thread on 2026-10-29.
  - `Symbols::new` (`keel-codegen/src/symbols.rs:24`) and `Module::defines`
    (`keel-ir/src/module.rs:138-145`) ignore `weak_odr` for functions and
    data.
  - So `symbol_addr` on AArch64 (`aarch64.rs:2290`) emits direct
    `PAGE21`/`PAGEOFF12` relocations.
  - keel-link rejects those for a coalesced, non-private-extern definition
    (`keel-link/src/write.rs:962-967`): "direct page reference to
    imported/coalesced symbol requires GOT relocation".
  - The ELF test (`keel/tests/elf_link.rs:322`) does not cover this path.
  - Possible flux workaround: emit `weak_odr hidden`, because keel-link
    coalesces private-extern definitions statically (`lib.rs:519`, `575-583`).
    The `.flib` branch uses it.
  - Fixed in keel `d3a0f24b` ("read exported weak_odr addresses from the
    Mach-O GOT"). flux pins keel `103c23de` since `ee1337a`.
- **Unwind records of a losing weak copy on Mach-O.** Observed by the
  `.flib` package work when building `memchr` as separate objects. I
  re-checked it in keel's source at `d17902eb` and at keel `main`
  `105e3d9e`, but have not reproduced it outside flux. Reported to the keel
  thread on 2026-10-29.
  - Two objects define the same weak function, and its unwind info needs
    DWARF mode.
  - The compact unwind record's function start is written as a section
    relocation, so it points at its own object's copy. `__compact_unwind`
    has `S_ATTR_DEBUG`, which `section_target` in `keel-asm/src/macho.rs`
    turns into a section relocation.
  - The FDE start is an external `SUBTRACTOR` pair against the function
    symbol, so it points at the copy the linker kept.
  - keel-link keeps the first weak definition and leaves the losing copy's
    code and records in place (`keel-link/src/lib.rs` about 313-327).
    `unwind_entries` (`unwind.rs:322`) then finds no FDE at the losing
    copy's address: "DWARF-mode compact unwind entry has no relocated FDE".
  - Under `dead_strip`, `gc.rs` drops a coalesced-away definition's metadata
    only when its relocation is external, so it misses this case too.
  - Fixed in keel `e4c72445` ("keep each weak copy's unwind records with
    its own code"): keel-link resolves unwind relocations to the defining
    object's copy. flux pins it since `1867228`.
- **`weak_odr` functions are never inlined.** Found by the `.flib` work.
  Reproduced at keel `e4c72445` with a two-function `.kir` file. Reported to
  the keel thread on 2026-10-29.
  - `eligibility_reason_with_frames` (`keel-opt/src/inline.rs:713-717`)
    rejects `f.weak_odr` before it checks `admit_noninterposable`. A hidden
    `weak_odr` helper marked `alwaysinline` and `no_semantic_interposition`
    stays a call. The same helper without `weak_odr` is inlined.
  - An ODR definition promises an equivalent body in every copy, which is
    the promise `InlineNoninterposable` asks for. LLVM inlines its
    `linkonce_odr` and `weak_odr` functions too.
  - With per-crate objects, flux emits generic instances as hidden
    `weak_odr`, so none of them are inlined. This is part of the
    `tree_pipeline_selfhost` slowdown at the Full tier on the `.flib`
    branch: 13.7 s on main, 38.0 s on the branch. 62% of profile samples
    land in `weak_odr` functions.
  - Fixed in keel `dcd44219`. The inliners accept a `weak_odr` callee that
    carries `noninterposable` or `no_semantic_interposition`. flux pins
    keel `103c23de` and marks its instances `no_semantic_interposition`
    since `ee1337a`. The reproduction now goes from 1 direct call to 0.

### Missing keel features

| # | flux location | Keel gap | Keel source | Cost to flux | Keel change |
|---|---|---|---|---|---|
| K1 | `lower.rs:615-649`, `1561-1569`, `2210-2272` | No aggregate values | `keel-ir/src/layout.rs:4-6` | Enums carry every variant's leaves; memory reads and writes go leaf by leaf | Aggregate values, or the planned `field.addr` layouts |
| K2 | `lower/abi.rs:407-425`; `lower.rs:3691-3701`, `3913-3926` | One return value | `verifier.rs:1956`, `aarch64.rs:7337-7340` | Every 2+ leaf result (`&str`, slices, `Option<&T>`, tuples) goes through sret; keel-opt leaves sret as is | Multiple returns for fast and C conventions |
| K3 | `lower/abi.rs:61-62`, `222-224`, `410-413` | Empty byval/sret rejected | `verifier.rs:1970` | 3 special cases | Accept and drop size-0 layouts |
| K4 | `lower.rs:2413-2419`, `2437-2444`, `4181-4187` | No pointer constants | `verifier.rs:1044-1047` | `iconst` + `inttoptr` for every null; `ptrtoint` for null tests | `iconst.ptr` |
| K5 | `lower.rs:2595-2613` | `iconst` takes an i64 immediate | `keel-ir/src/inst.rs:16-17` | 5 instructions per wide u128 constant | i128 immediate |
| K8 | `library/core/src/sync/atomic.rs:125-147`, `331-353` | CAS takes one ordering and returns only the old value | `builder.rs:294-298` | Orderings joined; extra compare; weak CAS is strong | CAS with success and failure orderings and a success flag |
| K9 | `lower.rs:4220-4235`, `3480-3486` | Default load flags promise align 1 and no `notrap` | `memory.rs:6-25` | Every access states alignment; DCE keeps dead loads (`keel-opt dce.rs:59-61`) | Infer `notrap` for stack addresses; flux sets it where it knows |
| K10 | `lower/abi.rs:121-161`, `241-272` | Natural C layout only | `layout.rs:13-19` | packed, `repr(align)`, unions, NEON wrappers and data enums rejected at C boundaries | Explicit size, alignment and offsets; unions; vector layouts |
| K11 | `lower.rs:3050-3097`, `3104-3247` | `fptosi`/`fptoui` only to i32/i64; no i128 int/float conversions | `verifier.rs:1852`, `1897-1900` | About 7 instructions per narrow cast, about 50 per i128 cast | Saturating conversions to i8/i16/i128; i128 `sitofp` |
| K13 | `lower.rs:152-164`, `3272-3280` | No float remainder | `inst.rs:431-463` | A libcall per `%`, never constant-folded | `frem` |
| K14 | `lower/abi.rs:380-396`; `lower.rs:3574-3578` | `cc(fast)` functions cannot be address-taken or called indirectly | `verifier.rs:441-444`, `721-724`, `1311-1314` | Address-taken functions use the C convention on every call; causes the `#[track_caller]` bug | `symbol_addr` of a fast function returns a keel-generated C thunk |
| K15 | `lower.rs:3350-3365`, `2066-2082` | `switch` takes i8 to i64 with i64 cases | `verifier.rs:1528-1532` | flux lowers wider switches to compare chains (`6fb5502`) | i128 `switch` |
| K16 | `lower.rs:1069-1078`, `1099-1114` | Data relocation addend is u32 | `data.rs:73-76` | Addends of 4 GiB or more, or negative, are rejected | i64 addend |
| K17 | `lower.rs:2829-2849`, `2874-2877` | Asm `w` operands must be floats; no vector types or `v` modifier | `verifier.rs:846`, `aarch64.rs:7518-7524` | Integers bitcast to floats; `vreg` without a modifier fails (`docs/SELF_HOSTING.md:513-514`). At `f37f0bd`, `tests/run/vector_inline_asm.rs` fails with "asm operand 0 (`w`) cannot be i8x16"; it passes at `d17902e`. Vector operands and `v` work since keel `782993ec`; integers in `w` operands are still rejected at `62584dcb` | Accept integer operands |
| K18 | `lower.rs:1605-1621`; fresh slots at `3437`, `3769-3777`, `3789`, `3822` | Every slot gets its own frame bytes. `lifetime_start`/`lifetime_end` exist but act only as optimizer barriers | `stack_frame.rs:26-33` | Frame grows with the number of call sites | Lifetime markers and slot coloring |
| K19 | `crates/flux-hir/src/cfg.rs:95-96` | No i128 atomics | `verifier.rs:779-781` | `target_has_atomic = "128"` is false | CASP-based i128 atomics |
| K20 | `lower.rs:1811-1817` | memmove/memset are libc calls; inline expansion only up to 32 bytes and only in the Balanced and Full tiers | `small_memory.rs:7`; `keel-opt/src/lib.rs:396`, `421-438` | Every array copy at -O0 is a libc call | Expand in every tier |
| K21 | per-crate objects; built-in `--native-object` suite | `dead_strip` keeps or drops whole input sections, and every function is in one `__text` | `keel-link/src/gc.rs:85` | Unused public functions of each dependency stay in every executable. A native built-in suite makes hello 2.3 MB instead of 84 KB, so the suite ships as MIR | `MH_SUBSECTIONS_VIA_SYMBOLS` atoms or one section per function. Requested 2026-10-29 |
| K22 | `library/core/src/arch.rs:534-590` | No AArch64 SHA-256 intrinsics; only `X86Sha256msg1`, `msg2` and `rnds2` exist | `keel-ir/src/intrinsic.rs:202-205` | `vsha256hq_u32`, `vsha256h2q_u32`, `vsha256su0q_u32` and `vsha256su1q_u32` are emulated with scalar lane code. `sha2` 0.11 detects the hardware and takes that path: 184 MB hashed as keel-link does (whole image plus 4 KiB pages) takes 1.71 s built by flux, 0.95 s with `sha2`'s software backend, and 0.052 s built by rustc. Measured 2026-11-05; it is about 1.7 s of the flux-built compiler's 4.5 s link of `fluxc` | `sha256h`, `sha256h2`, `sha256su0` and `sha256su1` intrinsics, lowered like the x86 ones |
| K23 | `crates/flux-codegen/src/native.rs` on `feat/build-script-linking` | Mach-O shared libraries can only come from `.tbd` files: `Dylib` is built only by `Dylib::from_tbd` | `keel-link/src/tbd.rs:9`, `46` | A `.dylib` or framework binary outside the SDK, such as a Homebrew or Nix library, cannot be linked; flux reports "linking the Mach-O shared library … is not supported yet". Found 2026-11-05 | Read a Mach-O dylib's exports and install name |
| K24 | same | No force-load option | `keel-link/src/lib.rs:38` | `+whole-archive` on `rustc-link-lib` is rejected; no registry crate uses it yet | A whole-archive flag per input archive |
| K25 | same | Every archive member built with debug info warns "DWARF input omitted; request --debug-map or --dsym" | `keel-link/src/lib.rs:401` | `cc` builds debug info in debug builds, so every C archive warns | Drop the warning for archive members, or warn once per link |

The highest-payoff keel changes are K2 (multiple returns), K9 (`notrap`)
and K18 (slot lifetimes); together they remove most of the memory traffic
the leaf model adds.

### Inlining and copy expansion, measured 2026-11-05

The pinned keel was measured through the flux-built compiler on
`perf/enum-memory`. That branch keeps enums with more than 48 leaves in
memory. It cut the stage-1 self-build by 23% in instructions retired.

Two flux workarounds stand in for keel changes:
- flux expands 33- to 128-byte copies itself, which gives about 6% of that
  gain;
- 18 tiny library accessors are marked `#[inline(always)]`.

Both could go once keel changes as follows:

1. **Copy expansion limit.** `keel-opt/src/small_memory.rs:7` expands
   fixed-size copies only up to 32 bytes; anything larger becomes a libc
   `memmove` call. Expanding up to 128 bytes in keel, in 16-byte chunks,
   would let flux drop its own expansion. Doing it after inlining would
   also keep it out of the inline cost.
2. **Module growth budget.** The budget is
   `max(196,608, entry IR / 2)` (`inline.rs:37`), and `inline.rs:1573`
   refuses every site once it is spent. That includes callees no larger
   than their call and `alwaysinline` sites. So smaller input IR gets less
   inlining: with the 48-leaf threshold, `Kinds::get` still calls
   `UnsafeCell::get` and `Option::as_ref` out of line. Keel should not
   charge sites that cost no more than the call, or should charge net
   growth.
3. **Inlining order.** Calls cloned in by inlining are not reconsidered in
   the same pass. A tiny `alwaysinline` leaf inside an inlined wrapper waits
   for a later round, when the budget may be gone: `Ty::entry` still calls
   `Option::copied` and `OnceLock::get`. Inlining callees before callers
   fixes this.
4. **Priority rules.** Callees that contain any intrinsic are excluded from
   the tiny-leaf, cheap-helper and aggregate-leaf paths
   (`inline.rs:412-450`, `681`). That includes a small `memmove`, which keel
   expands inline anyway. Plain `#[inline]` gets priority only for
   multi-block callees that store (`inline.rs:1353`), which is why the
   library needed `inline(always)`.

### flux restrictions that keel already supports

These are flux-side gaps, not keel's.

- Inline asm in MIR is tied in/out only (`crates/flux-mir/src/lib.rs:556-563`,
  `build.rs:3020-3052`). keel has separate outputs and `m` and `i` operands
  (`keel-ir/src/asm.rs:21-44`).
- u8 atomics are limited to load, store and CAS, and u16 to load and store
  (`crates/flux-mir/src/mono.rs:2211-2213`). keel does read-modify-write on
  any integer up to i64 (`inst.rs:135-160`).
- `read_unaligned` and volatile are scalar-only (`mono.rs:447-488`). keel's
  `MemoryFlags` carry alignment and volatility on any load.
- `ptr_add` round-trips through an integer (`mono.rs:1394-1421`). keel has
  `Ptradd` (`inst.rs:453`).
- Function pointers to trait-object methods, references and slices in
  `extern "C"` signatures, and wide-pointer comparisons are rejected
  (`crates/flux-ty/src/infer/coerce.rs:258`, `infer.rs:3193`,
  `signature.rs:923`, `941`, `infer.rs:7259`). Not confirmed to be keel
  limits.

## Performance

### Fixed on `perf/integration`

The local branch `perf/integration` (not on `main` yet) merges `perf/driver`,
`perf/lowering-caches`, `perf/solver`, `perf/codegen` and `perf/enum-memory`.
Measured on 2026-11-05, arm64 macOS, single runs:

| Build | `9eabfc31` | `6590599a` (without `perf/enum-memory`) | `546c8712` |
| --- | --- | --- | --- |
| stage 0 (rustc) builds stage 1 | 238 s | 92 s | 76 s |
| stage 1 (flux) builds stage 2 | 1,344 s | 523 s | 262 s |
| peak memory, stage 2 build | 18.3 GB | 18.8 GB | 16.3 GB |

The baseline stage 0 used the `test` profile; the others use the new
`stage0` profile. At `546c8712`, built in one source directory, stage 1 and
stage 2 are byte-identical (`02684c51…`). The complete suite passes all 480
targets. 60 `tests/run` programs, with and without `-O`, compile to
byte-identical executables with stage 0 and stage 1 (checked at
`6590599a`).

These entries below are fixed there:
- uncached `holds`, `needs_drop` and `select_impl` (both exponential cases);
- `probe` copying `vars`;
- uncached `is_memory`, `leaves` and `leaf_count`;
- uncached symbol names and definition keys;
- uncached field types;
- `line!` and panic locations scanning from offset 0.

Not in the entries below, and also fixed there:
- proc-macro host files hashed twice per expansion;
- host crates built one at a time into a new directory per run;
- `debug_assertions` always on;
- bit-count methods implemented as loops;
- derived comparisons that test each variant in turn;
- `Copy` slices copied element by element.

- enums with more than 48 leaves are kept in memory, with copies of 33 to
  128 bytes expanded inline (see "Inlining and copy expansion" under the
  keel issues).

Parallel lowering and parallel keel compilation are still open; in the
stage-2 build, keel takes about 138 of the 262 s and lowering 32 s. The
remaining entries in this section were not measured.

### Trait solving

- `holds` caches nothing and explores shared types as a tree
  (`crates/flux-ty/src/traits.rs:2097-2206`; `needs_drop` at `2480-2502` has
  the same shape). `struct A29(A30, A30); ...; struct A0(A1, A1);
  static S: Option<A0> = None;` makes about 2^30 calls for the `Sync`
  check. observed: release `flux mir` takes 0.98 s at depth 20, 2.6 s at 22
  and 9.0 s at 24, in `not_sync` -> `holds_at`. Fix: cache by (environment,
  goal), storing only results that did not depend on a cut cycle.
- Region proofs for iterator chains are exponential in the chain length.
  observed: release `flux mir` on a chain of `.map(|x| x + 1)` calls takes
  1.1 s for 10 adapters, 6.8 s for 12, 61 s for 14, and more than 120 s for
  16. rustc compiles 40 at once. The hot path is borrowck's `prove_bound` ->
  `ordinary::prove` -> `requires_lifetime_proof` -> `select_impl` ->
  `may_hold` -> `holds_at`.
- `select_impl` and `normalize` cache nothing and allocate a candidate
  `Vec` and an `FxHashMap` per call (`traits.rs:1770-1816`, `1570`). The
  thread-local `ACTIVE` stack makes answers depend on the call site and is
  not unwound on panic. Fix: pass the stack explicitly and memoize on the
  interned `Predicate`.
- `check_overlap` recomputes `coherence_header_ty` for every pair
  (`traits.rs:5271-5274`); `regions::slots` is recomputed at every node of
  the walkers, O(n²) in type size (`regions.rs:413`).

### Type inference

- `probe` copies the whole `vars` vector per probe
  (`crates/flux-ty/src/infer/solve.rs:296-307`). Probes run per candidate
  impl, per trait in scope and per autoderef step. Fix: an undo-log
  snapshot.
- Every ambiguous obligation is selected again on every
  `select_obligations` call (`solve.rs:312-432`, about 20 call sites). Fix:
  record which variables a selection waits on.
- The `proven` cache is rebuilt per `may_hold` call (`solve.rs:1452-1454`),
  so `String: Deref` is proved again at every method call; `exists_unmet`
  runs at every autoderef step though it only matters on error
  (`infer.rs:4951-4956`).
- `known`, `occurs`, `unknown_vars` and `resolve` walk the type DAG without
  a memo (`infer.rs:835-855`, `6723-6737`, `6914-6944`); nested shared
  tuples cost 2^depth.
- Environments are cloned per goal (`solve.rs:605`, `1104`, `1242`,
  `1736`), and fixed names like `"deref"` are interned on each use
  (`infer.rs:4412`, `4271`, `5864`).
- Inference-time types go into the global append-only table and are never
  freed (`crates/flux-ty/src/ty.rs:779-791`, `727`).

### MIR

- Or-patterns multiply exits: a tuple of n `0|1` fields gives 2^n blocks
  (`crates/flux-mir/src/build.rs:4674-4702`, `4518-4525`). Each arm
  re-reads the discriminant; there is no decision tree (`5007-5011`).
- `body_of` is not cached and runs twice per instance (`mono.rs:722-734`;
  `flux-codegen/src/lower/abi.rs:332`, `lower.rs:217`). `subst_body`
  normalizes every type with no memo (`mono.rs:2039`).
- `Layouts::new` runs per call site, per body and per recursive query
  (`mono.rs:474`, `493`, `1133`; `alignment.rs:61`; `eval.rs:1800`;
  `flux-codegen/src/lower/abi.rs:295`). Fix: one program-wide cache.
- `Builder::read` calls the uncached trait solver for `is_copy` on every
  ADT read (`build.rs:1668-1675`).
- Drop glue is inlined at every drop site rather than built once per type
  (`mono.rs:1749-1780`, `363-382`).
- Const eval clones on move and then copies through `Arc::make_mut`
  (`eval.rs:2523`, `2571`, `3689-3700`), so array updates through a
  `const fn` are O(N²). `has_union` and `resolve_instance` repeat per call
  (`1884-1910`, `2343`).
- The init dataflow pops a stack instead of iterating in reverse postorder
  and runs twice (`init.rs:90-113`). `lens.rs:31-47` re-infers every
  constant each round. `Projection::appended` copies the chain per push
  (`projection.rs:42-44`).

### Borrow checker

- `close_known` recomputes an n×n transitive closure with repeated O(n³)
  passes, from more than 30 call sites (`borrowck/regions.rs:2989-3022`).
  Fix: one Warshall pass with bitset rows.
- `escapes` runs a BFS per loan eagerly but is only read on error
  (`regions.rs:232-239`, `8466`).
- `overlaps` interns a new `Projection` per prefix and recomputes
  `place_ty` from the root (`borrowck.rs:435-439`).

### HIR and syntax

- The import fixpoint re-resolves every import and re-copies every glob's
  scope each round; pending body macros clone the whole `DefMap` per retry
  (`crates/flux-hir/src/def_map.rs:1088-1154`, `1442-1537`;
  `item_tree/collect.rs:761-773`).
- O(n) scans per event: `line!` and panic locations scan the file from
  offset 0 (`macros/mod.rs:707-720`); each diagnostic scans all files
  (`item_tree.rs:1688`); each macro definition scans all macros
  (`collect.rs:548-553`); duplicate associated item checks are quadratic
  (`def_map.rs:1765-1808`).

### Codegen

- Results of two or more leaves go through sret memory (K2), and leaves
  ignore niches: `Option<&T>` is two leaves, so every slice iterator's
  `next` returns through memory. Fix: treat niche-filled enums as their
  memory scalar (rustc's ScalarPair), plus K2.
- No flux load sets `notrap` (`lower.rs:4230-4235`), so dead loads survive
  `-O`. Loading an enum reads every variant's fields (`2241-2255`). A
  parameter of more than 16 leaves is reloaded in full at entry
  (`1575-1579`; threshold `lower/abi.rs:431-433`).
- A `const` array is rebuilt on the stack at every use
  (`lower.rs:2328-2332`). Fix: use the read-only `promoted()` object.
- `is_memory`, `leaf_count` and `leaves` recurse on every call;
  `Signatures::fields` re-substitutes and `variants` allocates per call
  (`crates/flux-ty/src/lib.rs:361-387`). Fix: a per-type cache on `Ctx`.
- Niche decoding builds one select per variant (`lower.rs:2185-2195`);
  rustc uses one add. Enum stores build a block per data variant
  (`2066-2089`).
- `[0u8; 4096]` becomes a byte-store loop (`lower.rs:2276-2304`). Without
  `-O` flux runs keel's empty None tier (`crates/flux-codegen/src/lib.rs:396`
  at HEAD).
- Copy coalescing is off in any function with a loop (`lower.rs:1491`).
  Every link spawns `xcrun` (`lib.rs:169-185` at HEAD).
- Per-crate objects are slower than one module (since `0cb3e84`). The
  `tree_pipeline_selfhost` program runs in 13.5 s at Full on `456acf4` and
  36.2 s on `0cb3e84`. At None it takes 37.3 s and 45.5 s. Both measurements
  are back to back on the same fixture. On `ee1337a` it runs in 14.9 to
  15.9 s at Full against 13.6 s whole-graph, and about 48 s at None, also
  unchanged from `0cb3e84`. The causes:
  - Generic instances are hidden `weak_odr`, which keel did not inline
    (see Keel bugs). Fixed by `ee1337a`, which accounts for 20 s of the
    Full-tier gap.
  - Non-generic functions in another crate's object have no body in the
    module, so they can't be inlined. `docs/FLIB_INLINING.md` proposes
    local copies for `#[inline]` and small functions.
  - Calls between objects use `cc(c)`, because keel's `cc(fast)` needs a
    local definition. Keel advised against a new convention. Its planned
    `cc(c)` leaf work covers most of what `cc(fast)` adds.

### Pipeline

- Compile time against rustc, on the tree fixture at `f5a7390` (report:
  `out/flib-reports/rustc-compare/report.md`). A clean four-crate build takes 1.18 s,
  against 1.01 s for rustc on one job and 0.66 s for Cargo's default jobs.
  An edit that touches only the root crate takes 0.66 s, against 0.11 s for
  Cargo. Follow-ups, none scheduled:
  - The root crate takes 0.45 s against 0.15 s. It compiles every generic
    instance and links. Not profiled yet.
  - Every program pulls in about 3,150 of 6,858 built-in bodies through
    conservative callee expansion. Demand-driven roots would make on-demand
    MIR loading worth building. The suite still loads in about 145 ms.
  - flux has no incremental cache within a crate. rustc's keeps Cargo's
    single-file edits 4 to 6 times faster.
  - Cross-crate inlining (`docs/FLIB_INLINING.md`) is parked. It is
    estimated to recover tenths of a second of the 1.85 s runtime gap.
- Nothing runs in parallel (`crates/flux-cli/src/build.rs:161-230`;
  `lower_module` at `lower.rs:216-227` takes `&mut Ctx`). Data object names
  come from `data.len()` and `vtables.len()` (`lower.rs:1224`, `1276`,
  `1330`), so output depends on order. Fix: name data by content hash and
  give each thread its own tables.
- The "mono + keel lowering" timing combines collection, two body
  substitutions and lowering. Split it before choosing what to optimize.

## Test suite

From a read-only review on 2026-10-29, at commit `3706a32`. The status note
below and the "Where the time goes" list were updated after the per-process
library cache (`114db09`..`746cce4`) and the runner changes
(`2b99b62`..`c65f047`) landed on local `main`. Times come from the shared
18-core machine, so they vary with load.

**Status:** a full `scripts/test.py all` took 1,599 s for 421 targets before
those changes. It took 1,121 s with the library cache at 4 jobs, and 302 s
for 430 targets with both changes (2,826 target-seconds, 14 jobs). The
duplication and most of the fragility below remain. They are the next
cleanup steps: one data-driven oracle harness, and merged self-host tests.

### Where the time goes

- The runner now starts 14 targets at once on 18 cores, longest first by
  the previous passing run. Only the 10 scaling and solver-limit targets
  run alone, for 11 to 28 s. The pool is limited by total work, not by one
  long target. Each target takes about 1.4 times as long as at 4 jobs.
- The slowest targets in the 302 s run: `flux-cli::run` 185 s,
  `mir_selfhost` 117 s, `tree_pipeline_selfhost` 102 s,
  `self_adt_constants` 93 s, `ty_selfhost` 83 s, `keel_asm_readiness` 82 s.
- Library work is now done once per process (`flux-ty/src/library.rs`,
  `flux-mir/src/library.rs`). A later in-process compilation of an empty
  `main` takes about 15 ms instead of 255 ms. Tests that start a new `flux`
  process per program still pay about 0.5 s of library work each time:
  - `tests/run` builds 474 programs once with rustc and twice with flux
    (plain and `-O`).
  - Every per-program oracle test in flux-cli does the same.

  Shipping the built-in library as a `.flib` would remove that cost.
- The fixture harnesses for `ty` and `mir` run their cases one at a time.
  The every-prefix fuzz runs three times, over 10,819 prefixes each
  (`parser.rs:101`, `hir.rs:89`, `ty.rs:234`).

### Duplication

- 83 flux-cli test files only call `support::oracle::compare_fixture` for a
  `tests/run` program, and return at once in a full run
  (`support/oracle.rs:19-22`). Each is still a separate binary.
- 136 flux-mir files paste the oracle harness inline: 148 blocks in 128
  files, 75 distinct after normalizing identifiers. In flux-cli there are
  58 metadata-oracle blocks, 39 inline native-compare blocks, 66 copies of
  the `for optimized in [false, true]` loop, `success` defined 22 times and
  `bounded` 20 times, although `support/process.rs` exists.
- The four fixture harnesses copy `fixtures()`/`check_dir()`, the prefix
  fuzz and `bytes_read`, about 170 of their 963 lines.
- `ty_selfhost.rs` and `mir_selfhost.rs` are 96% identical text. The hir,
  ty and mir graphs nest, as do lexer, parser and `tree_pipeline`.

### Fragility

- `--changed` finds the tests that name a `tests/run` program by text
  search, so it can select a few extra tests. An edit under
  `tests/run/support` still selects all 203 flux-cli tests.
- Self-host and readiness tests hard-code crate versions and graphs, for
  example `ty_selfhost.rs:103-117` and `keel_asm_readiness.rs:110-149` (13
  pinned versions with features). A `Cargo.lock` bump fails as a bare
  `unwrap()` (`ty_selfhost.rs:132`). A new dependency fails the edge
  assertion.
- Several tests cut production sources at doc-comment markers
  (`run.rs:182-185`, `lexer_selfhost.rs:97-100`, `parser_selfhost.rs:98-109`,
  `tree_pipeline_selfhost.rs:99-110`). Editing one doc comment breaks them.
- Any std API that flux's own `alloc` lacks breaks the hir, ty and mir
  self-host tests together, as `String::drain` did.
- Deadlines:
  - 98 fixed process deadlines of 30-90 s in 61 files, plus 7 elapsed-time
    assertions, can trip under load. At 14 jobs every target runs about
    1.4 times slower. Only the self-host limits were raised (`ty_selfhost`
    300 s per run, `mir_selfhost` 600 s).
  - The runner's own limit per target is five times its last passing
    duration, at least 300 s, or 900 s with no record.
  - `run.rs` compiles have no timeout.
- `out/` in the main checkout is 51 GB. The runner now keeps the 10 newest
  run directories. `run.rs` and the self-host tests use
  `tests/support/scratch.rs`, which removes a passing test's directory. The
  per-file oracle and readiness tests still leave theirs, for example 11 GB
  of `out/keel-backend-readiness`. A test killed by a timeout keeps its
  directory.
- `[profile.test]` sets `opt-level = 1`, and the dev profile does not. Both
  write the `flux` binary to `target/debug/flux`, the path the tests
  run. The workspace `flux check` step in `just check` uses `cargo run`, so
  it builds the whole compiler a second time with the dev profile and
  replaces that binary. On 2026-10-29 the `.flib` thread ran that step
  during a full suite. The binary changed from 32 MB to 115 MB mid-run, and
  targets timed out one after another. `cargo run --profile test` would
  reuse the suite's build and leave the binary alone. Since `2e9e945` the
  swap also makes the staged built-in suite stale, because the suite is
  keyed to the executable's SHA-256. Each later `flux` process then
  compiles the built-in library from source until `build-builtins` runs again.
- Symbols in per-crate output depend on the compiler executable's bytes.
  The built-in crates' hashes are the SHA-256 of the compiler binary plus
  the crate name (`flib/semantic.rs`, `DependencyState::new`, at `ee1337a`).
  Every `__flux_definition_<hash>` symbol chains from them. Two builds of the
  same flux source, such as the test and dev profiles, give one program
  different symbol names. On 2026-10-30 the tree fixture built by both
  profiles at `ee1337a` differed in 4,451 of 4,453 symbols, at the same
  size. One compiler binary builds deterministically. Byte comparisons and
  runtime pairs need the same compiler binary, and symbol order may shift
  code layout. rustc mixes in its version string, not the binary.
- The rule that `ok/` fixtures compile with rustc is not checked by any
  harness, and `err/` diagnostics are never compared with rustc's.
- The rustc oracle floats with `stable` (`rust-toolchain.toml`). For
  example, `float_math.rs:268` records rustc 1.99.1 behavior, and 9 files
  assert rustc's message text.
- 12 case rows expect flux to disagree with rustc (for example
  `foreign_statics.rs:123-139`, `const_pointer_views.rs:283-306`), spread
  across files rather than in one list.
- Snapshot churn: in the last 200 commits, 7 commits changed 98 snapshots
  whose fixture was untouched. MIR dumps number locals and blocks
  sequentially, and changes to how `core` is lowered ripple into many
  `.mir` files.
- CI compiles out the 151 macOS-arm64-only test files on Linux. The macOS
  job runs only the fast plan and `run`, so the self-host and readiness
  tests run only locally.

## Code quality and architecture

### Several trait solvers

`holds_at`, `SignatureImplProof`, `may_hold`/`select_impl`,
`coherence_goal_impossible` and `infer/solve.rs` each have their own depth
limits and meaning of "ambiguous". `borrowck/regions.rs` has another (the
`impl_region_goal*`, `impl_prove_super*`, `prepare_bound`, `caller_bound`,
`prove_step` and `families` code). Trait lifetime admission runs five
profiles (`flux-ty/src/regions.rs:2246-2251`, `regions/trait_profile.rs`).
The coherence, `?Sized`, recursion-limit and where-clause entries above
come from these disagreeing. One cached solver over interned predicates,
with explicit cycle and depth state, a single overflow result and a
coherence mode that uses only knowable negative evidence, would remove
that class and the caching problem.

### Paired check and commit functions

`unify`/`can_unify` (`crates/flux-ty/src/infer.rs:930-1042` vs
`1141-1216`) and `coerce`/`can_coerce` (`infer/coerce.rs:10-381` vs
`578-717`) are decision trees kept in step by hand, and they have drifted.
With a cheap snapshot, each `can_x` becomes `probe(|cx| cx.x(..))`, about
250 fewer lines. `probe` also rolls back only `vars` and `projections`;
diagnostics, obligations and side tables written inside a probe survive
(`solve.rs:296-307`, `509-528`).

### Region slot layout written out about eight times

`flux_ty::regions::slots` and `instantiate_at`, `RegionRow`, `relate_at`
(`borrowck/regions.rs:4093`), `sinks` (`7730`), `closures_in` (`7565`),
`late_slots::positions` (`5736`), `storage_live_slots` (`7856`),
`drop_live_vars::visit` (`7965`) and `place_regions`. The copies already
disagree. Every new slot-bearing `TyKind` needs about eight edits, and a
missed one shifts indices silently. Fix: one cursor yielding (slot,
variance, path).

Implicit invariants in the same file: `drop_live_vars` adds variables in
the middle of `solve`, so the order at `8155-8156` matters;
`universal_regions` returns early (`2290-2320`) and relies on an error
already being pushed.

### No MIR visitor

Operand walks are written out in `crates/flux-mir/src/lib.rs:980-1085`,
`mono.rs:226-234`, `subst_body` (`mono.rs:2067-2146`) and `init.rs`. Side
tables keyed by block index are how the `call_regions` bug happened.
`Rvalue::Cast` is documented as integer-to-integer (`lib.rs:566-569`) but
carries pointer, fn-pointer, float and `CStr` casts; `validate.rs` checks
only the target type.

### Incrementality and parallelism blockers

- The item tree expands macros inside bodies and stores each expansion as
  a new file with a sequential `FileId`, so editing one body changes the
  item tree (`crates/flux-hir/src/item_tree/collect.rs:2350`, `2437-2533`).
  Expansions with diagnostics are expanded twice (`2497`).
- Hygiene numbers come from process-wide counters
  (`macros/mod.rs:159-180`).
- `ItemTree::resolve` pushes to a mutex-guarded `loaded_externs` list during
  resolution, and flux-ty's `std_available` reads it, so in a `no_std`
  crate a body's methods depend on lowering order
  (`item_tree.rs:1545-1573`, `def_map.rs:766-770`, `lower.rs:2310-2325`,
  `flux-ty/src/infer.rs:1928`).
- Macro type lowering uses thread-locals keyed by tree address
  (`item_tree.rs:3157-3258`).
- The reachability worklist is sequential (`crates/flux-ty/src/lib.rs:1488`),
  and side interners sit behind single global mutexes (`ty.rs:284-395`,
  `type_identity.rs`). `Ty` indices depend on interning order, so results
  cannot be cached across processes without a structural encoding.

### Large functions and duplication

- Derives are about 1,450 lines of hand-built HIR across six builders
  (`crates/flux-hir/src/lower.rs:227-1680`; `derived_ord_body` alone is 345
  lines). Generating tokens and reusing the expansion path would remove
  them. Also large: `collect_fn` (455 lines), `collect_enum` (360), `derive`
  (300).
- flux-ty: `infer_expr_inner` (about 635 lines), `check_pat_in` (470),
  `finish` (415), `run_deferred` (390); `Ctx` has about 90 fields.
- flux-mir: `lower_value` (650 lines, `build.rs:3016`), `collect_all`
  (about 490, `mono.rs:74-561`), which also hard-codes NEON, atomic and
  `read_unaligned` names (`394-513`).
- flux-codegen: `terminator` is 400 lines (`lower.rs:3322-3723`), and the
  "jump to target or trap" tail is copied 9 times. `write_bytes`
  (`1022-1185`), `value_leaves` (`2432-2540`) and `store_value`
  (`2360-2402`) must agree on `eval::Value` layout. The packed-alignment
  update is repeated 5 times (`1669`, `1905`, `1964`, `2133`, `2216`).
- Two `array_unsize` implementations, one comparing with `==` and one
  unifying (`crates/flux-ty/src/metadata.rs:58`, `infer/coerce.rs:723`).
- The tokens to text to parse pipeline is copied three times
  (`crates/flux-hir/src/macros/mod.rs:827`, `910`, `1109`).
- Joint and contextual-keyword marking is duplicated
  (`crates/flux-syntax/src/lib.rs:205-225` and `Input::from_lexed`).

### Smaller issues

- `report_unresolved` picks its message by substring search, so
  `use crate::superfluous::x;` gets the wrong message
  (`crates/flux-hir/src/def_map.rs:1321-1333`).
- `c_void` is special-cased by string comparison in four resolution paths
  (`def_map.rs:212`).
- Auto traits are special-cased by lang names ("cell", "unsafe_cell",
  "phantom_pinned", "phantom_data") in place of negative impls
  (`crates/flux-ty/src/traits.rs:2213`, `2286-2312`).
- `resolve_instance` falls back silently when selection fails for a
  concrete goal (`traits.rs:2547-2563`); it should be an internal error.
- `ArenaMap` derives `PartialEq` over its offset and padding, so equal maps
  can compare unequal (`crates/flux-arena/src/typed.rs:364`).
- `flux-intern/src/lib.rs:215-216` increments `next` before asserting it is
  below `u32::MAX`; with poisoning ignored, the next call wraps to symbol 0.
  Memory-safe; needs 4 billion symbols.
- `extern crate self as x` reports "can't find crate for `self`" instead of
  "not supported yet" (`crates/flux-hir/src/item_tree/collect.rs:2986`).
- `"Output"` is looked up by string in borrowck
  (`borrowck/regions.rs:7340`, `regions/callable_outputs.rs:20`, `49`);
  use a predefined symbol.
- `Return::Sret` and `Return::Memory` are never told apart
  (`crates/flux-codegen/src/lower/abi.rs:400-405`); `returns.len() <= 1` at
  `abi.rs:111` is always true; five `DataObject` literals could use
  `Default`.

### Stale docs

- `crates/flux-codegen/src/lib.rs:12-13` says a failed assertion calls
  `abort`.
- `docs/ARCHITECTURE.md` says lint attributes cannot change a program's
  meaning, but `#[allow]` on a deny lint does.
