# Roadmap

flux has three goals: understand how a Rust toolchain works by building one,
measure where Rust's compile-time and runtime costs come from, and use what
it learns to design a language of its own. It also exists to stress
[keel](https://github.com/hxyulin/keel) with Rust-shaped code.

## 1. Syntax: done

Lexer, error-recovering parser and lossless flat syntax trees for a Rust
subset. See [the supported language](LANGUAGE.md).

## 2. Typed syntax and item structure

- Done: a typed AST over `SyntaxNode` (`ast::Fn<'t>`, `ast::Expr<'t>`,
  ...), generated from `flux.ungram`, with trees checked against it.
- An *item tree*: each file's items with their signatures but not their
  bodies, and stable IDs for items based on their path rather than their
  offset. Editing one function body must not change any other item's ID.
- Diagnostics with error codes and `--json` output, so tools and coding
  agents can consume them.

## 3. Semantics

Started: name resolution, type inference, MIR and the part of borrow
checking that needs no references (initialization and moves) for
functions over integers, `bool`, raw pointers, tuples and structs. In
order next:

1. `impl` blocks and methods on user types, with references to locals
   (`&self`, `&mut self`) and the borrow checking they need.
2. Crates and modules, to compile flux's own `core` (below): `mod`,
   multi-segment paths, the prelude, and `#[lang]` items and intrinsics,
   allowed only in `core`.
3. Generics and monomorphization, then traits: operators on user types,
   `Copy`/`Clone`, `Drop`, `for` loops over `Iterator`.

Each comes with fixtures that rustc must agree with.

## 4. Code generation through keel

Started: functions over integers, `bool`, raw pointers, tuples and
structs compile to arm64 macOS executables through keel, checked against
rustc by `tests/run`. Aggregates are split into scalar variables, so they
need no memory until references exist. Next:
linking on x86-64 Linux, and a way to compare keel's output with
Cranelift's and LLVM's on the same IR.


Lower the IR to `keel-ir` in-process using keel's SSA builder, starting with
`panic=abort`. Since keel has Cranelift and LLVM bridges, the same IR can be
compiled by all three backends to compare compile time and runtime on
identical input, especially on code where Rust relies on inlining and
scalar replacement of aggregates (iterators, closures, `Option`).

## A core library instead of std

rustc's `std` and `core` depend on compiler internals (lang items,
intrinsics, unstable features), so flux cannot compile them. flux will
provide its own small core library, written in flux, that defines the same
names (`Option`, `Result`, the operator traits, `Copy`, `Sized`) with the
same meaning. Output goes through `extern "C"` declarations of libc
functions. Test programs written against that shared surface compile under
both flux and rustc, which keeps rustc usable as an oracle: compare exit
codes and output.

## Incremental and parallel checking

The aim is a warm daemon that rechecks only what an edit affects, at the
granularity of a function body. Incrementality starts after parsing: a
changed file is reparsed whole, which takes milliseconds, and the item
tree decides what else must be redone. The remaining pieces:

1. **Stable item IDs.** Name items by path, not by their position in the
   tree, and hash each item's signature and body separately, so an edit
   to one body leaves every other item's ID and hashes unchanged.
2. **Signature/body split.** Type-check against the item tree only.
   A body edit that leaves its signature unchanged cannot affect other
   items, so only that body is rechecked.
3. **Query-based caching.** Each analysis is a function of its inputs,
   memoized and invalidated by dependency tracking (salsa-style, or a
   smaller purpose-built version), recording what each body read.
4. **Parallelism.** Files parse independently; bodies type-check
   independently given the item tree. A skeleton parse that skips bodies by
   brace matching would also let bodies be parsed in parallel and on demand.

## Other ideas

- A faster lexer: it is now at 6 ns/token; keyword lookup through a perfect
  hash and fewer bounds checks are the next things to measure.
- Contextual keywords (`union`, `auto`, `default`, `safe`, `raw`) marked by
  the lexer in a side table, keeping the parser free of text.
- Fuzzing the parser (cargo-fuzz) for panics and lossy trees.
- A language server early on, on the same frontend as the compiler:
  lossless trees and error recovery are most of what one needs.
- Smaller trees: nodes are 16 bytes and outnumber half the tokens; a
  narrower token range or a separate table for large nodes could shrink
  them.
