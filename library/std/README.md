# Flux's bounded std facade

This embedded crate reexports facilities implemented by Flux's core and
alloc. It is not a complete Rust standard library. Imports such as
`use std::fmt`, `use std::iter::FusedIterator`, `use std::ops::Index`,
`use std::mem`, and `use std::vec::Vec` name the original definitions;
types and trait implementations retain their core/alloc identity.

The root exposes core's `any`, `arch`, `array`, `ascii`, `cell`, `char`, `clone`, `cmp`,
`convert`, `default`, `error`, `ffi`, `hash`, `hint`, `iter`, `marker`, `mem`, `num`,
`ops`, `option`, `panic`, `ptr`, `result`, `slice` and `str`, and alloc's
`alloc`, `borrow`, `boxed`, `fmt`, `prelude`, `string` and `vec`. Only the
APIs actually implemented in those modules exist. `cell` includes the existing
`Cell` and `RefCell` implementations, including sized borrow-guard dereferencing;
reexporting them does not add further standard-library APIs.
`std::arch` retains core's bounded architecture APIs, including the existing
AArch64 NEON operations used by Keel's delimiter scanner.
`arch::is_aarch64_feature_detected!("crc")` supplies runtime CRC detection on
macOS and Linux. The corresponding unsafe CRC intrinsics use exact scalar
folding, without hardware acceleration. Other feature queries and safe
target-feature functions remain outside the implemented surface. Native
execution is verified on arm64 macOS; Linux detection remains unverified.
`num` provides bounded `NonZeroU32` and `NonZeroU64` APIs. Their zero niches
preserve the corresponding Option layouts. Other NonZero types and APIs are
not implied.
`any` provides only `type_name<T: ?Sized>() -> &'static str`. The name is
emitted as a static diagnostic string after type substitution, using Flux's
diagnostic printer. Paths may be omitted; names are not unique and do not
promise rustc's spelling. This follows the diagnostic-only
[Rust API contract](https://doc.rust-lang.org/std/any/fn.type_name.html).
`Any`, `TypeId`, downcasting and `type_name_of_val` remain absent. The stable
function is not callable in constants.

`collections` reexports the bounded BTreeMap implementation from alloc.
`HashMap<K, V, S>` and `HashSet<T, S>` supply probing storage, borrowed lookup,
removal, entries, iteration and ownership traits. Their default `RandomState`
obtains keys from the operating system; clones preserve those keys.
`std::hash::DefaultHasher` supplies streaming SipHash-1-3. Its algorithm is an
implementation detail. Custom build hashers remain supported. Vec,
String and Box remain available at their usual module paths and through the
standard prelude. Printing macros and alloc's formatting macro also have real std
reexports so explicit imports work in no_std libraries that opt into std.

`thread` exposes `panicking() -> bool` and native `LocalKey` storage through
`thread_local!`. Initializers run once per thread, and POSIX thread-exit
destructors mark a key inaccessible before dropping its value. `with` and
`try_with` prevent callback references from escaping. Explicit, imported and
qualified std macro paths are supported. `panicking()` returns false. Flux's existing panic runtime
prints the panic and calls C's abort, without unwinding or executing
unwind destructors. Ordinary executing code therefore observes false.
`thread::scope` creates real pthread-backed workers with borrowed closures.
Scope exit joins outstanding handles and nested workers. Consuming `join`
returns the worker result, and worker panics abort in Flux. Native comparison
and host-unwinding tests exercise scope cleanup. Unscoped `thread::spawn`, panic
hooks, catch_unwind and an unwinding Flux runtime remain absent.

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
freestanding runtime. `#![no_core]` remains unsupported for user crates. Core and
alloc cannot see std or the program. Std has ordinary crate privacy. Its trusted
embedded root supplies the standard float inherent methods; user crates do not
gain primitive-implementation or intrinsic privileges from names or paths. Std
cannot define compiler lang items.

All embedded source files and macro expansions are cached together before
program files are added. The shared item tree and lazy body/signature
caches include std, so library file and item IDs stay stable across
compilations. Std is crate 2; source dependencies follow it in graph order,
then the executable crate. Without dependencies the program remains crate 3;
core and alloc retain crate IDs 0 and 1. No rustc library metadata is loaded.

`sync` reexports `Arc`/`Weak` from alloc, including supported slice ownership,
and core's bounded atomic module. `OnceLock` supports static construction and
single-winner initialization. `LazyLock` supports named functions and closures
in statics, dereferencing and `force`.
Sized `Mutex` and `RwLock` use acquire/release spin locks and guards that release
on destruction. They expose lock results, poison errors, exclusive payload
access and extraction. They do not promise fairness. The runtime aborts on panic;
poison behavior under unwinding is tested separately with rustc.
Native contention tests use an audited C ABI bridge to call Flux-generated
lock functions from host threads; this does not add Rust thread creation.
`Vec` and `Box` have Send/Sync implementations with element bounds.

Floating math includes `mul_add`, `sqrt`, integral rounding and integer powers
for both widths. Methods that Rust exposes through std require actual std
availability, including nested imports or loaded std-using dependencies.
Nondefault floating environments are not a general supported runtime contract;
the fixed ties-even test is separate from default-environment rustc comparisons.
`env` supplies target OS/family/architecture constants and UTF-8-key variable
queries. Non-Unicode results retain their Unix bytes through VarError::NotUnicode
and the bounded owned OsString. Borrowed OsStr and Path retain arbitrary Unix bytes through genuine unsized
wrappers. PathBuf owns its bytes; comparison and hashing normalize components.
The bounded filesystem API reads bytes/UTF-8, resolves canonical paths and tests
regular-file status through native Unix calls. Environment mutation, Windows
paths and the remaining file/directory APIs remain absent. `rc::Rc<T>` supports
sized and slice ownership, non-atomic cloning, unique mutation and sized
clone-on-write/extraction, and remains neither Send nor Sync.

`io` provides `Read`, `Seek`, `SeekFrom` and forwarding implementations for
mutable references. Reads from byte slices and the default `read_exact`,
`read_to_end` and `read_to_string` helpers handle short reads, interrupted calls,
EOF and UTF-8 errors. Seeking is supplied by the implementing stream; this does
not add `File`, `Cursor`, buffered readers or writing APIs. Error kinds retain
platform categories and formatting. Stability gating for the Rust-unstable
`FilesystemLoop`, `InProgress`, `TooManyOpenFiles`, `InputOutputError` and
`Uncategorized` variant names is not implemented.

`time` supplies a bounded `Duration` and `Instant` API for elapsed profiling,
normalized nanoseconds and checked arithmetic. Native timestamps use Apple's
`CLOCK_UPTIME_RAW` or Linux's `CLOCK_MONOTONIC` through `clock_gettime`.
Wall-clock time, sleeping, duration parsing and the remaining formatting APIs
are absent. Native execution is tested on arm64 macOS; Linux execution remains
unverified.
