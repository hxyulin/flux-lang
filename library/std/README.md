# Flux's bounded std facade

This embedded crate reexports facilities implemented by Flux's core and
alloc. It is not a complete Rust standard library. Imports such as
`use std::fmt`, `use std::iter::FusedIterator`, `use std::ops::Index`,
`use std::mem`, and `use std::vec::Vec` name the original definitions;
types and trait implementations retain their core/alloc identity.

The root exposes core's `any`, `array`, `ascii`, `cell`, `char`, `clone`, `cmp`,
`convert`, `default`, `ffi`, `hash`, `hint`, `iter`, `marker`, `mem`, `num`,
`ops`, `option`, `panic`, `ptr`, `result`, `slice` and `str`, and alloc's
`alloc`, `borrow`, `boxed`, `fmt`, `prelude`, `string` and `vec`. Only the
APIs actually implemented in those modules exist. `cell` includes the existing
`Cell` and `RefCell` implementations, including sized borrow-guard dereferencing;
reexporting them does not add further standard-library APIs.
`num` now provides the checked `NonZeroU32`
subset (`new`/`get`, comparison, copying, hashing and formatting), whose
zero niche preserves `Option<NonZeroU32>` at four bytes. Other NonZero
types and APIs are not implied.
`any` provides only `type_name<T: ?Sized>() -> &'static str`. The name is
emitted as a static diagnostic string after type substitution, using Flux's
diagnostic printer. Paths may be omitted; names are not unique and do not
promise rustc's spelling. This follows the diagnostic-only
[Rust API contract](https://doc.rust-lang.org/std/any/fn.type_name.html).
`Any`, `TypeId`, downcasting and `type_name_of_val` remain absent. The stable
function is not callable in constants.

`collections` reexports the bounded BTreeMap implementation from alloc. Vec,
String and Box remain available at their usual module paths and through the
standard prelude. Printing macros and alloc's formatting macro also have real std
reexports so explicit imports work in no_std libraries that opt into std.

`thread` exposes only `panicking() -> bool`. Flux's existing panic runtime
prints the panic and calls C's abort, without unwinding or executing
unwind destructors. Ordinary executing code therefore observes false.
This query does not add thread creation, panic hooks, catch_unwind or an
unwinding runtime, and must be reconsidered if those semantics change.

The new FusedIterator marker in core is implemented for Fuse, mutable
references to fused iterators, slice Iter/IterMut, and Vec IntoIter/Drain.
It is not yet implemented for every standard iterator or adapter.

Normal compilations load core, alloc, std, then the program. The existing
alloc prelude and macro behavior are unchanged; plain alloc paths still
require `extern crate alloc;`. Bare HIR/item-tree entry points still omit
all embedded crates from resolution. `#![no_std]` source library roots use
core's prelude and omit implicit std/alloc type and printing macro imports;
`extern crate alloc` and `extern crate std` opt into those libraries. A
no_std executable currently requires explicit std; this does not add a
freestanding runtime. `#![no_core]` remains unsupported for user crates. Core and alloc cannot see std or the
program. Std has ordinary crate privacy and cannot define primitive
implementations or compiler lang items.

All embedded source files and macro expansions are cached together before
program files are added. The shared item tree and lazy body/signature
caches include std, so library file and item IDs stay stable across
compilations. Std is crate 2; source dependencies follow it in graph order,
then the executable crate. Without dependencies the program remains crate 3;
core and alloc retain crate IDs 0 and 1. No rustc library metadata is loaded.

`sync` reexports sized `Arc`/`Weak` from alloc and the bounded core atomic
module (`AtomicUsize`, `Ordering`, and fences). These have real atomic
operations and ownership semantics; they do not provide mutexes, locks,
thread creation, or dynamically sized Arc payloads. Sized `UnsafeCell` is
also available through `cell`. Filesystem and the remaining runtime/library
APIs are absent. The NonZeroU32 layout support is implemented in core/compiler
code rather than simulated by facade aliases.
