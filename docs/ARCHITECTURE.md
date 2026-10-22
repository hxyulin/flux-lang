# Architecture

flux is a Cargo workspace:

| Crate | Role |
| --- | --- |
| [`flux-cli`](../crates/flux-cli/src/main.rs) | The `flux` binary: files, arguments, diagnostics, exit codes |
| [`flux-syntax`](../crates/flux-syntax/src/lib.rs) | Source text to lossless syntax trees, and the typed AST over them |
| [`flux-sourcegen`](../crates/flux-sourcegen/src/lib.rs) | Generates the typed AST from [`flux.ungram`](../crates/flux-syntax/flux.ungram) |
| [`flux-hir`](../crates/flux-hir/src/lib.rs) | Item tree and HIR: function bodies with names resolved |
| [`flux-ty`](../crates/flux-ty/src/lib.rs) | Type inference and checking |
| [`flux-mir`](../crates/flux-mir/src/lib.rs) | MIR: control-flow graphs built from typed HIR |
| [`flux-codegen`](../crates/flux-codegen/src/lib.rs) | MIR to keel IR, objects and executables |
| [`flux-intern`](../crates/flux-intern/src/lib.rs) | Global, thread-safe string interning (`Symbol`) |
| [`flux-arena`](../crates/flux-arena/src/lib.rs) | Bump allocation and typed index arenas |

Libraries do no I/O.

Later stages (name resolution, type checking, borrow checking, lowering to
[keel](https://github.com/hxyulin/keel) IR) will each be their own crate
consuming the one before; see the [roadmap](ROADMAP.md).

## Parsing

Parsing has three stages. Each is a plain function from owned data to
owned data, so each can run on any thread:

```text
text ──lexer──▶ LexedStr ──Input──▶ parser ──▶ Output ──build──▶ SyntaxTree
                (all tokens)  (kinds only)     (events)    (+ tokens)
```

1. **Lexing.** [`lexer.rs`](../crates/flux-syntax/src/lexer.rs) splits text
   into tokens, trivia (whitespace and comments) included, as a struct of
   arrays: a kind and a start offset per token. It never fails: malformed
   tokens carry an error, unknown characters become `ERROR` tokens.
   Punctuation is always a single character. Whether `>>` is a shift or two
   closing angle brackets is the parser's decision, so the lexer needs no
   context. It works on bytes and decodes a `char` only for non-ASCII
   input, and skips comment and string bodies with `memchr`.
2. **Parsing.** [`parser/`](../crates/flux-syntax/src/parser/mod.rs) reads an
   `Input`: the significant token kinds, plus a bit per token saying whether
   the next token touches it ("joint"). The parser glues joint punctuation
   into composite tokens (`::`, `>>=`) where the grammar wants them. It is a
   recursive-descent parser with precedence climbing for binary operators,
   and it emits a flat list of events (`Start`, `Token`, `Finish`, `Error`)
   instead of building a tree. It never sees text and never fails. An
   event is 8 bytes (error messages are kept in a side table), since a
   file produces about two per token.
3. **Building.** [`build.rs`](../crates/flux-syntax/src/build.rs) replays the
   events against the tokens into a [`SyntaxTree`](../crates/flux-syntax/src/tree.rs),
   placing trivia. Trivia goes before the next token, except that comments
   directly above an item attach to the item, so a doc comment belongs to
   what it documents. The tree reuses the lexer's arrays, merging composite
   tokens in place.

### Why events

A node's kind is sometimes known only after its first child: `a` becomes the
left side of `a + b` once `+` is seen. With events, the parser wraps a
finished node by recording a *forward parent* (`CompletedMarker::precede`)
rather than moving anything. `CompletedMarker::extend_to` does the reverse,
so attributes parsed before an expression end up inside its node.

Events also decouple the grammar from the tree library, and make the
parser's output independent of the text, which matters for reparsing a
single item.

### Error recovery

Every rule produces its node even when parts are missing: `expect` reports
a missing token without consuming anything, and `err_recover` wraps one
unexpected token in an `ERROR` node unless it is in a recovery set (tokens
that start items, or closing delimiters that belong to an enclosing rule).
List rules always consume a token or stop, and the parser asserts if it
looks at the same position too many times, so a grammar bug fails loudly
instead of hanging.

The result is always lossless: the tree's text equals the input, byte for
byte, for any input. The tests check this for every prefix of every fixture
(a stand-in for a file being typed), and the corpus test checks it across
large codebases.

### Trees

A [`SyntaxTree`](../crates/flux-syntax/src/tree.rs) is one file in four
allocations: the text, the token kinds, the token start offsets, and the
nodes. Nodes are stored in preorder, 16 bytes each: a kind, the range of
tokens they cover, and how many descendants follow them. A subtree is a
contiguous slice of the node array, so descendants are a range, children
are found by skipping subtrees, and a node's text is a slice of the
source. The tree is `Send + Sync` and has no internal pointers.

`SyntaxNode<'t>` and `SyntaxToken<'t>` are `Copy` handles: a reference to
the tree and a 32-bit index. Parents are not stored; `parent()` walks down
from the root. Later stages should refer to syntax by `NodeId` or by path,
not hold on to handles.

flux used rust-analyzer's [rowan](https://github.com/rust-analyzer/rowan)
green trees at first. They share unchanged subtrees between versions of a
file, which suits an editor reparsing on every keystroke, but every node
and token is a separate reference-counted allocation, and building them
took two thirds of parse time. flux's incremental design does not need
the sharing: it reparses a changed file whole (milliseconds even for large
files) and reuses work *after* parsing, keyed by item. The same tree is
meant to serve a language server too, rather than a second frontend.

## The typed AST

[`ast`](../crates/flux-syntax/src/ast.rs) gives the tree Rust types: a
struct per node kind (`ast::Fn`, `ast::BinExpr`) and an enum per position
(`ast::Item`, `ast::Expr`, `ast::Pat`, `ast::Type`). Each wraps a
`SyntaxNode`, is `Copy`, and finds its children on demand: nothing is
converted or allocated. Accessors return `Option` or iterators, because a
tree with errors can lack any part.

The node structs are generated from
[`flux.ungram`](../crates/flux-syntax/flux.ungram), which states what each
node may contain. Children that share a type and differ only by position
(`BinExpr::lhs` and `rhs`, an `if`'s condition and branches) are written
by hand in [`ext.rs`](../crates/flux-syntax/src/ast/ext.rs); the
generated code checks they exist. flux-sourcegen's tests check that every
tree parsed from the fixtures and the workspace (and, with `just corpus`,
every error-free file of a corpus) matches the grammar, so the AST and
the parser cannot silently drift apart.

Macro calls get a kind per position (`MACRO_CALL` for items,
`MACRO_EXPR`, `MACRO_TYPE`, `MACRO_PAT`) so that, for example, a `let`
statement's pattern, type and initializer stay distinguishable.

### Where type checking happens

"Typed" means the AST's nodes have Rust types, not that the program has
been type-checked. The AST mirrors the source, with every parenthesis and
spelling variant, which is right for diagnostics and tools and wrong for
analysis. Semantic analysis will follow rustc's and rust-analyzer's split:

1. **Item tree:** each file's items and signatures, with path-based IDs.
2. **HIR:** each body lowered from the AST into an arena of expressions
   and patterns with names resolved and sugar removed (`for` to `loop`
   and `match`), keeping a map back to AST nodes for diagnostics.
3. **Type inference and trait solving** on HIR bodies, one body at a time,
   given the item tree's signatures.
4. **MIR** for borrow checking and lowering to keel.

## HIR

[`flux-hir`](../crates/flux-hir/src/lib.rs) builds two things from a
program's files:

- The **item tree**: every crate's modules, and every function's,
  struct's, trait's, constant's and `impl` block's name, generic
  parameters, bounds and signature, with types and traits as written
  (`TypeRef::Path(u8)`, `Pair<T>`, `T: Tr<u8>`), but no bodies.
  Everything outside a body depends only on it. A method is a function
  owned by an `impl` block or a trait whose first parameter is `self`,
  with the type `Self`, `&Self` or `&mut Self`. Each trait knows its
  `impl` blocks, and each name the traits with a function of it.
  `#[derive(Clone)]` and friends become `impl` blocks here, after the
  crate's other items as in rustc, each with a method marked as derived
  (`Function::derived`), whose body is built from the struct's fields
  rather than lowered from syntax (for an enum, a `match` on its
  variants; for `PartialOrd` and `Ord` on one, a `match` that gives
  each variant's discriminant first, which values of different
  variants compare by, as rustc's expansion reads them). Structs and enums share one arena (`Struct`, with an
  enum's `variants`), since their generics, bounds, derives and `impl`
  blocks work alike; what a constructor, struct expression or pattern
  names is a struct or one variant of an enum, by index, and
  `Struct::variant` gives either's fields the same way. A variant's
  discriminant is a value when it is written as a literal or counted
  from one; one written as another expression (`A = 1 << 2`) is an
  anonymous constant (`Const::discriminant`) of the enum's `repr` type,
  lowered in the enum's module with `Self` the enum, and the variants
  after it are its value plus how many variants later they come
  (`Discriminant::Const`).
- The **def map** ([`def_map`](../crates/flux-hir/src/def_map.rs)): what
  each module's names mean, in Rust's two namespaces (types and values),
  from its items and its `use` imports. Imports are resolved together,
  repeating until nothing changes, since one can depend on another; a
  glob brings in every name visible to the importer, below explicit
  names, and a name two globs disagree on is an error where it is used.
  A path is resolved segment by segment through modules, checking each
  item's visibility from where the path is written; a name not in a
  module's scope comes from the prelude, then from crates (`core`), then
  from the primitive types. An enum's variants are in its own scope,
  before its associated items (`E::A`), and imports and globs can bring
  them into a module's (`use E::*;`). A block with items in a body is
  an anonymous module inside the item's (`Module::block`): a name not
  in its scope is looked up in the scopes around it, `self` and
  `super` skip it, and its private items are private to the module
  around it. Lowering and type checking a body resolve the names in
  such a block in its module (`Body::block_modules`).
- A **body** per function and constant, lowered into arenas of
  expressions, patterns and bindings addressed by `Idx`. Names are resolved while lowering,
  through a stack of scopes and then the item tree, so an expression
  `x` becomes `Res::Local(binding)` or `Res::Function(id)`, and `S::new`
  the associated function `new` of the struct `S`. A path whose item
  depends on types, such as `T::f`, `<T as Tr>::f`, `Tr::f`, or `S::f`
  for an `f` that `S`'s own blocks lack, is left to type checking
  (`Res::Assoc`). Generic arguments
  written in a path are kept by what they are for: `S::<A>::f::<B>`
  has `A` for `S` and `B` for `f`. Method calls stay
  unresolved until types are known. `break` and
  `continue` record the loop they leave. Parentheses disappear, and
  `while c { body }` becomes `loop { if c { body } else { break } }`, as
  in rustc, and `for pat in head { body }` becomes `match
  IntoIterator::into_iter(head) { mut iter => loop { match
  Iterator::next(&mut iter) { None => break, Some(pat) => body } } }`
  (`Body::for_loop_match` finds its parts again); ranges are `core`'s
  range structs, `a..=b` a call of `RangeInclusive::new`. Loops may
  have labels, which `break` and `continue` resolve to their loop. A
  closure (`Expr::Closure`) is part of the body it is written in: its
  parameters and bindings are the body's, its body sees the bindings
  around it but not the loops, and `return` in it returns from it. A
  `let` in the condition of an `if` or `while`, alone or in
  a chain of `&&`, stays an expression (`Expr::Let`), as in rustc's HIR,
  whose bindings the `then` branch sees. Each alternative of an
  or-pattern binds the same bindings, by name, so a binding is one local
  however it matched. A source map gives every node's text range for
  diagnostics.

A program has two crates: flux's own `core`
([`library/core`](../library/core/src/lib.rs)), built into the compiler
and compiled with every program, and the program itself, whose files are
read as its `mod` declarations name them (`m.rs` or `m/mod.rs`). Both
crates' items share one item tree, with each module knowing its crate.
Only `core` may write `impl` blocks for primitive types, which is how
`i32::MAX` and `x.abs()` exist.

`core` cannot name the program, so its part of the front end is the
same in every compilation, and is done once per process, the first
time it is needed: [`Core`](../crates/flux-hir/src/library.rs) holds
its files, parsed in parallel, which are the first files of every
`SourceDb`; its item tree, with names resolved; its constants' values;
and its function bodies, lowered when first asked for (a program uses
few of them). A program's item tree starts as a copy of `core`'s and
adds the program's crate after it, so `core`'s items keep their ids,
and `ItemTree::core` says where they end. The copy is cheap: item
arenas are [`LayeredArena`](../crates/flux-arena/src/typed.rs)s, whose
frozen part, `core`'s items, is shared through an `Arc`, and only the
tables a program may add to (the def map, each type's associated items,
each trait's `impl` blocks) are copied. Resolving the program's names
touches only its own modules, imports and `impl` blocks.

Bodies depend only on the item tree, so they lower independently (and
in parallel, later). Syntax outside the [supported subset](LANGUAGE.md)
becomes `Expr::Missing` with a "not supported yet" error; flux never
silently ignores code, and ignores only attributes that cannot change a
program's meaning (`allow`, `inline`, `doc`, ...).

## Types

[`flux-ty`](../crates/flux-ty/src/lib.rs) resolves every signature from
the item tree, then infers each body on its own against them. `core`'s
signatures are resolved once per process, from `Core`'s item tree, and
a program's are resolved after a copy of them, in the same order, so
they see the same `core` as if everything were resolved together (a
test checks this for every fixture). `core`'s `impl` blocks are checked
against their traits by its tests, not on every build.

A `Ty` is a 4-byte handle into one process-wide table of `TyKind`s, like
`Symbol` for names: types compare and hash as integers, and a type made
of types (`*const T`) stays `Copy`. The primitive types are interned
first, so `Ty::BOOL` and `Ty::int(IntTy::U8)` are constants. Interning
takes a lock, but reading a type's kind does not: kinds live in
write-once slots of an append-only table, as symbols' strings do.

A generic item's signature mentions its parameters (`TyKind::Param`, by
index, an `impl` block's before a function's own), and a struct type
carries its arguments (`Pair<i32>`). Each use of a generic item gets new
variables for its parameters, or the types written for them
(`id::<u8>`), and `Ty::subst` gives the use's signature. A call of an
associated function also unifies its block's type (`Pair<T>` in
`impl<T> Pair<T>`) with the type it is called on, so blocks for
different instances of a struct (`impl Pair<i32>`, `impl Pair<u8>`) may
each have a function of the same name; the one whose block may be for
the receiver's type is called, and two are E0034. Each call records the
function and generic arguments it calls. A parameter has the methods of
its bounds' traits, no operators or fields, and is `Copy` only if a
bound says so.

A trait's items are generic over its parameters with an implicit
`Self` first, so a call of a trait's method, `x.m()` or `Tr::m(x)`, is
a call of the trait's function with `Self` the receiver's type. The
call's bounds (and `Self: Tr` itself) become *obligations*, met by a
bound in scope or by the one `impl` block of the trait whose header
unifies with it, whose own bounds become obligations in turn; one with
an unknown `Self`, or more than one block that may apply, waits until
types are known, and integer variables take their default before what
is left is reported (E0277, rustc's spans). Probing for a method tries
each type the receiver dereferences to, inherent methods first, then
the traits in scope whose blocks may apply, bounds included; trying an
`impl` block unifies in a snapshot of the variables, rolled back
after.

An associated type is an alias, `<T as Tr>::Out` (`TyKind::Alias`),
*normalized* to what it is where that is known: from a bound in scope
(`T: Tr<Out = i32>`) or from the `impl` block for its types. One whose
`Self` is not yet known becomes a variable, replaced when it is; one of
a type parameter without a binding stays, a type of its own. The bounds
in scope are those written with their supertraits; an associated type's
own bounds (`type Smaller: Dimension;`) are found from its trait when a
goal, method call or normalization names it, as rustc's *item bounds*
are, since they nest without end. Operators
on types other than integers and `bool`, references to them included,
are calls of their traits' methods, found through `#[lang]` names (`a +
b` is `Add::add(a, b)`, its type `<A as Add<B>>::Output`), and so are
monomorphized like any other trait call. As rustc's
`enforce_builtin_binop_types` does, where the operands are integers
past one shared reference each, they must have the same type (except
for shifts), and the result has it, before the trait's `impl` block is
known. `<` on references, through `core`'s one `impl PartialOrd<&B>
for &A`, is selected before its right operand is checked, which must
then be a `&B` (E0308), as in rustc; `==` has two such impls, so a
mismatch there is the unmet `A: PartialEq<B>` (E0277). Comparisons of
references call the referents' method with the references as they are
(`compared_refs`). In constants and `const fn`s, a call of a trait's
method is E0658 where the `impl` block is one of `core`'s for its own
types, which rustc's `core` makes conditionally `const`, and E0015
otherwise.

A closure's type (`TyKind::Closure`) is unique to its expression and
its owner's generic arguments, followed, as rustc's are, by three
types inferred like any other: its kind (`i8`, `i16` or `i32` for
`Fn`, `FnMut` or `FnOnce`), its signature, and the tuple of what it
captures. Its signature comes from a call's `Fn`-family bounds on the
parameter it is passed to (arguments that are closures are inferred
after the others, as in rustc), which also fix its kind, or from its
annotations, body and calls. Once the body is inferred
([`closure`](../crates/flux-ty/src/infer/closure.rs)), each closure,
innermost first, has its captures found as rustc's edition-2021
analysis finds them: each use of a place outside it (a binding and
fields and dereferences into it, an index stopping at the array) is a
copy, move, borrow or assignment; moved places are captured by value
and others by reference (all by value with `move`), cut at the first
dereference by value and after the last shared one by reference, and
a place used whole and in part is captured whole. Its kind follows
from the uses, and so do the methods its calls call (`f(x)` is
`Fn::call(&f, (x,))`, `FnMut::call_mut` or `FnOnce::call_once`) and
whether the bounds it was expected to meet hold (E0525). The solver
meets `Fn`-family bounds on closures, function items and pointers
from their signatures, and normalizes `FnOnce::Output` to their
return types.

`x?` stays an expression in HIR (`Expr::Try`), since what it does
depends on `x`'s type: `core`'s `#[lang = "Option"]` and `#[lang =
"Result"]` enums are the only types with `Try` on stable Rust that flux
has. Type checking matches rustc's `Try` and `FromResidual` bounds
with the function's return type, and for a `Result`, records the
`From::from` call that converts the error, an obligation like any
other; MIR building tests the variant and returns the rest. Outside bodies, where every type is known up to rigid parameters,
[`traits`](../crates/flux-ty/src/traits.rs) answers whether a predicate
holds by matching `impl` headers, which checks supertraits, overlapping
blocks (E0119, with bounds that rule types out) and blocks' methods
against their traits (E0046, E0053, ...).

A struct's bounds are not assumed where it is named, as in rustc: each
type in a signature must meet the bounds of the structs it names, and
each associated type its trait (`<T as Tr>::Out` needs `T: Tr`), given
the item's own bounds (checked by `traits`, each unmet bound once per
item, where rustc reports it), and in a body, a type
written in an annotation or path, and the type a constructor or struct
expression builds, adds them as obligations, reported at the value
whose field's type has the bound's parameter, as rustc points. A field
may name a bound's associated type (`made: S::Out`); field types are
normalized once the struct's arguments are known.

An associated constant of a trait is generic over the trait's
parameters, `Self` first, and one of a generic `impl` block over the
block's; a path to either (`T::N`, `<S as Tr>::N`, `Self::N`) records
its generic arguments, as a call does, with `Self: Tr` or the block's
bounds as obligations.

An array's length is part of its type (`TyKind::Array(T, 3)`), so
the constant a length names (`[u8; N]`) is evaluated while types are
resolved, as rustc's queries do: type checking takes a function that
evaluates constants (`check_file_with`; [`flux_mir::check_file`](../crates/flux-mir/src/lens.rs)
gives it one that infers them, builds their MIR and runs it), resolves
signatures once with the lengths it knows (those written as literals,
or named by constants whose values are literals, need no evaluation),
evaluates the constants that were missing and resolves them again,
and infers a body again if it named one only it does. A constant whose
value needs its own length is a cycle.

Where the type an expression must have is known before it is inferred
(a `let` with a type, an argument, a field, a return value), it is
noted, and a call or struct expression unifies its result with it first,
as rustc does: `let x: u8 = id(1i32)` is a mismatch at `1i32`.

Inference is unification over type variables, structural for types that
contain types:

- An unsuffixed integer literal gets an *integer variable*, which only
  unifies with integer types and defaults to `i32`. As in rustc, a
  literal directly under `as` takes the cast's type, so `300 as u8` is an
  out-of-range literal.
- `!` coerces to any type rather than unifying with it, so
  `if c { 1 } else { return }` is an integer. A variable that only `!`
  flows into falls back to `!` (edition 2024's never-type fallback).
- Checks that need final types run after inference: literal ranges
  (rustc's deny-by-default `overflowing_literals`), the operand types of
  operators, casts, and unary `-` on unsigned types.

A method call `x.m()` looks through `x`'s references for the struct with
a method `m`, and records how the receiver is passed: dereferenced so
many times, then borrowed for `&self` and `&mut self` or passed by
value. A field access dereferences the same way. Where a `&mut T` is
used as a reference (an argument, or a `let` with a type), it is
reborrowed rather than moved, and may become a `&T`, as in rustc.

A pattern is checked against the type of the value it matches. One that
is not a binding, a wildcard or a reference pattern sees through the
references of that type first, and its bindings then bind by reference
(rustc's default binding modes, recorded per binding and per pattern
for MIR). After inference, a body without errors gets the checks rustc
makes on patterns ([`pats`](../crates/flux-ty/src/pats.rs)): the
usefulness algorithm of Maranget's "Warnings for pattern matching", as
rustc implements it, finds whether a `match` is exhaustive and a `let`
or parameter pattern irrefutable. Patterns are constructors applied to
fields (variants, `true` and `false`, one for a struct, tuple or
reference, and ranges for integers, split at the bounds the patterns
of a column use, with an infinity past `isize` and `usize`'s ends), and
what is not covered is reported as witnesses printed as rustc prints
them. A pattern that names a constant (`N`, `0..i32::MAX`) is an
expression, typed like one; it matches the constant's value, which
type checking does not know. So the checks of a body with one wait:
`flux_mir`'s `build_file` evaluates the constants its patterns name,
from their MIR, then runs them (`check_deferred_patterns`), and does
not borrow-check the bodies they fail in, as rustc does not.

Without lifetimes, references are checked only where each one lives in a
single local assigned once: parameters, temporaries, and non-`mut`
`let` bindings with a value. A reference anywhere else (in a tuple or
struct, returned, in a `mut` binding) is reported as not supported yet.

It also makes the checks rustc makes before borrow checking: calling an
`unsafe` function or dereferencing a raw pointer needs an `unsafe` block,
a constant or `const fn` may call only `const fn`s, a field, method or
type must be visible where it is used, and assigning a local needs `mut`,
except for the first assignment of a `let x;` binding. Telling a first assignment from a second (and use
before initialization) needs control flow, so that check moves to MIR.

## MIR

[`flux-mir`](../crates/flux-mir/src/lib.rs) turns each type-checked body
into a control-flow graph: basic blocks of assignments to locals, each
ending in a terminator (`goto`, `switchInt`, a call, an assertion,
`return`). `_0` holds the return value and `_1..=n` the parameters, and
every intermediate value gets a temporary. This is where Rust's runtime
semantics become explicit, so code generation never sees expressions:

- `+`, `-`, `*` and unary `-` wrap, as rustc does without overflow
  checks, and shift amounts are masked to the left operand's width.
- `/` and `%` are preceded by assertions that the divisor is not zero
  and, for signed types, that the operation is not `MIN / -1`. rustc
  checks these even in release builds; a failed assertion aborts.
- `a[i]` copies `i` into a temporary, asserts that it is less than the
  length (an array's, or a slice's from the pointer it is behind),
  and is the place `a[_n]`; a move out of it is E0508, and borrowck
  takes any two elements of one array to overlap, as rustc does.
  `[x; N]` is one rvalue, `Repeat`; `len` reads the length (`Len`), as
  a borrow of its receiver; an unsizing coercion is an `Unsize`
  rvalue. Drop glue drops an array's elements in a loop, and
  `Clone::clone` and `PartialEq::eq` of arrays, which the compiler
  implements, get MIR made by `mono`.
- `&&` and `||` branch, and reading a local copies it into a temporary,
  so `x + { x = 10; x }` adds the old `x`.
- A call to a function returning `!` has no successor block.
- Values are read from *places*, a local and a path of field indices
  and dereferences (`(*_1).0.2`), and each read is a copy or, for a type
  that is not `Copy` (asked of the trait solver, with the function's
  bounds, for structs and parameters), a move. `&place` makes a reference to a place, and
  a method call is a plain call whose first argument is the receiver,
  dereferenced and borrowed as type checking decided. A block's
  bindings get a `StorageDead` where it ends, including where `break`
  and `continue` leave it.
- Values are dropped (a `drop` terminator) where their scope ends:
  bindings at the end of their block, in reverse order, then
  parameters; temporaries at the end of their statement, of a block's
  tail expression (before the block's bindings, as in edition 2024) or
  of a condition, unless a `let` borrows them, which moves them to its
  block's scope; and a place before it is overwritten. `break`,
  `continue` and `return` drop what they leave. Only types that may need
  it are dropped: a struct with a `Drop` impl or such a field, and a
  type parameter that is not `Copy`. Tuples, structs and enums' variants are built whole by an aggregate rvalue;
  a pattern like `let (a, S { b, .. }) = t;` reads each binding's part of
  `t` directly, so it moves only those parts, as in rustc.
- An enum's fields are those of one variant, a *downcast* away
  (`(_1 as Some).0`), and which variant it holds is its discriminant,
  read by an rvalue. A pattern is a sequence of tests of the parts of
  the place matched that it looks at (a discriminant, a comparison with
  a literal or a range's ends), each going on or to where matching
  fails, and then its bindings, each assigned from its part of the
  place, or a reference to it. A test of a discriminant names the
  variant, since one that is a constant's is known only once constants
  are evaluated, which may run this very body; a unit variant cast to
  an integer (`E::A as i32`) is its discriminant, a literal or its
  constant plus the variants since, as in rustc. An or-pattern has a way of matching per
  alternative, each with its own bindings and the next alternative to
  resume at, so a guard that fails tries it. A `match` tests its arms in
  order; a guard reads the parts its arm would move instead of moving
  them, and runs before they are moved. `if let` and `let`-`else` are
  the same tests, with the scrutinee's temporaries and the bindings
  dropped before the `else` branch runs, as in edition 2024.
- A function item's value has no data: its type says which function
  it is, so a call of one is a direct call. Coerced to a pointer, it is
  a constant (`Constant::Fn`), and a call through a pointer is a
  `CallIndirect` terminator.
- A static is a place behind a temporary that holds its address, as in
  rustc's MIR: `&T`, or `*mut T` for a `static mut`, so `X.a` is
  `(*_3).a`. Borrow checking sees it behind a pointer, and names it as
  rustc does ("cannot assign to immutable static item `X`"); constant
  evaluation reads it as its initial value.
- `x?` moves `x` into a temporary, as rustc's `Try::branch(x)` takes
  it, and tests its variant: the `Some` or `Ok` value is its result;
  otherwise `_0` becomes `None`, or `Err` with the error passed through
  `From::from` (unless the types are the same), and the function
  returns, dropping what is in scope as `return` does.

Only reachable code is built: every function of the program, and the
functions of `core` that something calls. A generic function is built
and checked once, in terms of its parameters; its calls record their
generic arguments in terms of its own. A constant's value is MIR too,
which [`eval`](../crates/flux-mir/src/eval.rs) runs at compile time, with
any `const fn` it calls, so constants support every expression form.
Division by zero is an error there (E0080, with rustc's messages), and
so is overflow in a constant's own initializer ("attempt to compute
`i32::MAX + 1_i32`, which would overflow"). A `const fn`'s arithmetic
wraps, as rustc compiles functions with flux's `-C overflow-checks=off`,
unless it is marked `#[rustc_inherit_overflow_checks]`, as `core`'s
`abs` and `pow` are: then it is checked where its caller's is. `core`'s
`#[lang = "panic"]` function aborts at run time; evaluation reports a
call of it as rustc's "evaluation panicked", with its message. Uses of a
constant refer to it with its generic arguments, and code generation
emits its value. The value of a trait's constant, or one of a generic
block, depends on its arguments: for them, it is the `impl` block's
constant or the trait's default (`traits::resolve_const`), which is
evaluated with the arguments passed on to the constants and `const fn`s
it uses. As in rustc, the constants evaluated are the free ones and
those that code without generic parameters names, with the arguments
it gives, whether that code is used or not; errors (E0080) are reported
at the constant that failed, and the lint below knows their values.
Discriminants written as expressions are evaluated before the other
constants, as rustc checks enums first, as far as each use needs, as in
rustc: a cast of a unit variant needs that variant's, but making a
value of the enum or running a body that matches on it needs them all,
so a discriminant that needs itself is a cycle (E0391), reported once,
at the discriminant it started from. Then the values of each enum are
checked for E0370 and E0081, and kept in `FileMir::discriminants` for
code generation and MIR dumps.
Those that generic code names are evaluated by code generation for the
instances it builds, so their errors come from `flux build` only, as
rustc reports them only in a full build; more than 128 constants deep
is rustc's "queries overflow the depth limit!".

[`lint`](../crates/flux-mir/src/lint.rs) reports what rustc's
deny-by-default `arithmetic_overflow` and `unconditional_panic` lints do,
even with overflow checks off: `let x: u8 = 255; x + 1` does not compile.
Like rustc's `KnownPanicsLint`, it propagates known values once through
the blocks in reverse postorder: a local assigned once keeps its value
everywhere, one assigned more often only in the block that assigned it,
and a borrowed one never has a known value. A negated literal is one
constant, as in rustc, so `-128i8` does not negate 128.

[`init`](../crates/flux-mir/src/init.rs) is the first piece of borrow
checking. It tracks *move paths*, each user local and every field path
into one that the body mentions, through three forward dataflow analyses:
which paths may be uninitialized, which may have been moved out of, and
which may have been assigned. Moving or assigning a path does the same to
every path below it. From these it reports, with rustc's wording, uses of
uninitialized bindings (E0381) and of moved or partially moved values
(E0382, once per set of moves, at the most specific use, as rustc
does), assignments to a field of a binding that is not fully
initialized or not `mut` (E0381, E0594), and assigning an immutable local
twice (E0384). Reading an enum's discriminant reads all of it. It runs
before drops are elaborated, as rustc's does, since elaboration reads
the discriminants of enums with fields moved out.

[`borrowck`](../crates/flux-mir/src/borrowck.rs) checks borrows with
rustc's non-lexical lifetimes, simplified by flux's rule that a
reference is assigned to a local only once. Each borrow is a *loan*;
the local it is stored in holds it, and so does any local a copy or a
reborrow of that reference goes into. A loan is live wherever a local
holding it is live (may still be read), found by backward liveness. Each
read, move, assignment and borrow of a place is checked against the live
loans of places that contain it or are contained in it, so borrows of
different fields never conflict, giving rustc's E0499, E0502, E0503,
E0505 and E0506. A method receiver's `&mut` is two-phase: shared until
the call, so `s.set(s.get())` is accepted. A loan still live where its
local's scope ends is E0597. The pass also reports `&mut` borrows of
non-`mut` bindings and anything written or mutably borrowed behind a
`&` (E0596, E0594), and moves out of a reference (E0507) or of a field
of a struct with a `Drop` impl (E0509), which moves nothing, so the
value is not also "moved" for `init`. A drop reads the value, so it
keeps the references in it live, and a temporary dropped while a
reference to it is still to be used is E0716. A closure that captures
by reference holds the loans of its captures, and so does any value
it is moved into: an aggregate holds its operands' loans, a call's
result those of its arguments if its type may hold any (a `Map` of a
closure), a reference to such a value the value's, and a value written
through a `&mut` (by assignment, or by a call it is passed to with
the `&mut`) gives its loans to the local the `&mut` borrows. Borrowing
such a value uses it. In a closure's body, what it captures is a
field of `self`, through the reference it captured unless by value,
and is named and checked as the captured variable (E0594 and E0596 for
an `Fn` closure's, E0507 for moves out of an `FnMut` closure's).

[`drops`](../crates/flux-mir/src/drops.rs) then *elaborates* the drops, as
rustc's `ElaborateDrops` does: with `init`'s analysis over the dropped
locals, where a drop moves out of its place, and a fourth bit for
whether a path holds a value now, each drop of a value moved out on
every path goes; one of a value initialized on every path stays; one of
a value moved out on some paths only is guarded by a *drop flag*, a
`bool` set where the path is assigned and cleared where it is moved out
of, dropped, or goes out of scope; and a struct or tuple with some fields
moved out is dropped field by field, each the same way (a struct with a
`Drop` impl cannot be, by E0509). An enum with fields of a variant moved
out drops the fields of each variant if it holds that variant. In a constant or `const fn`, a drop
that may find a value, even part of one, is E0493, as rustc checks
before elaborating: `Drop` impls cannot run at compile time.

## Code generation

[`flux-codegen`](../crates/flux-codegen/src/lib.rs) does everything after
MIR in-process with [keel](https://github.com/hxyulin/keel): MIR to keel
IR, keel's optimizer (`-O` runs its full pipeline), keel's code generator
and assembler to a native object, and keel's linker to a signed
executable. No C compiler or system linker is involved.

- Each MIR local becomes keel variables, one per scalar *leaf* of its
  type in field order (a `(i32, (bool, u64))` is three), and keel's
  `FunctionBuilder` builds SSA form, so MIR's temporaries and whole
  aggregate copies cost nothing. A field is a range of its local's
  leaves.
- A local that is borrowed lives in a stack slot instead, laid out as a
  C struct of its fields, and so does whatever a pointer points to:
  reading or writing a place there loads or stores each leaf at its
  offset.
- A value that contains an array is indexed at run time, so it lives
  in memory whatever happens to it: its locals are stack slots, its
  value is the address of one, copied with `memmove` where it is
  assigned, it is passed to a function as that address (the callee
  copies it) and returned through `sret` memory laid out as the value
  is. An element is at its index times its size. A reference to a
  slice is two leaves, the address of the first element and the
  length, which an unsizing coercion adds to a reference to an array
  and `len` and bounds checks read.
- An enum is its discriminant, an `isize`, then every variant's fields
  in turn, as if a struct of them all; in memory, a struct of the
  discriminant and a struct per variant. Only the variant it holds has
  meaningful leaves. Variants could share their leaves, but nothing can
  observe an enum's size yet.
- Aggregates are passed as one argument per leaf. keel functions return
  at most one value, so a function returning two or more leaves returns
  them through an `sret` pointer to a C struct of the leaves in the
  caller's frame; keel returns small ones in registers.
- `bool` is an `i8` holding 0 or 1; `()` and `!` have no value and
  disappear from signatures. Narrow integers are extended by signedness
  at calls, as Apple's arm64 ABI requires of C callers.
- keel takes shift amounts modulo the width, which is exactly Rust's
  masked shift; the amount is converted to the shifted value's type.
- Pointers, including `&CStr`, are keel `ptr`s. Each distinct C string
  becomes a module-local, read-only data object ending in a NUL byte,
  and each static a data object holding its evaluated initial value,
  laid out as in memory, writable only for a `static mut`.
- A function pointer is a keel `ptr`, a function's `symbol_addr`, and
  a call through one keel's `call_indirect` with C's convention. Since
  keel allows `cc(fast)` for direct calls only, monomorphization
  collects the instances whose address is taken, and those use C's
  convention everywhere.
- Calls to C-variadic functions use a variadic keel signature, which
  gives the platform's convention (on Apple arm64, variadic arguments go
  on the stack).
- Rust functions get mangled symbols from their whole path
  (`_ZN3fib3fibE`, `_ZN4core3num3i323absE` for `core::num::i32::abs`), so
  they cannot collide with C names or each other. The C `main` is a wrapper that calls the Rust
  `main` and returns 0. A failed MIR assertion calls `abort`.

Generic functions are *monomorphized*:
[`mono`](../crates/flux-mir/src/mono.rs) collects the instances the
program needs, starting from `main` and following calls (so, as in
rustc, a function nothing calls is not built), with each call's arguments substituted by its caller's,
as rustc's collector does; one more than 128 calls deep is rustc's
"reached the recursion limit" error (`f::<(T, i32)>` calling itself).
Each instance is a copy of the function's MIR with its arguments
substituted and its drops expanded into *drop glue*, a call of
`Drop::drop` with a borrow of each part of the value whose type has a
`Drop` impl, the struct before its fields and fields in order, and for
an enum the fields of the variant it holds, which it tests (the
collector follows these calls too), compiled like any other function, and its symbol is the
function's with a hash of the arguments as the last segment, as in
rustc's legacy mangling. A call of a trait's method stays one in MIR;
once its types are known, `traits::resolve_instance` makes it a call of
the method of the `impl` block for them, or of the trait's provided
method with `Self` substituted, as rustc's `Instance::resolve` does.
A closure's body is MIR of its own (`FileMir::closures`), built from
its expression as the method of its kind's trait, `call(&self, args)`
with the arguments as one tuple, generic in its owner's parameters,
with *shims* for the traits beyond its kind (`call_once` of an `Fn`
closure borrows it, calls `call`, and drops it); a call of
`Fn::call::<C, A>` for a closure type `C` is its body, substituted with
the arguments `C` holds (`FileMir::instance_body`). A closure is an
aggregate of what it captures, so code generation needs nothing new
for it. Function items and pointers get shims that call them, and a
closure that captures nothing becomes a pointer to its `call_once`,
whose arguments are those of the pointer's signature, leaf for
leaf.

Only arm64 macOS can link so far: Linux needs glibc's startup objects
passed to keel's ELF linker. `flux build --timings` prints each phase's
time.

## Names and memory

Identifiers will be stored as [`Symbol`](../crates/flux-intern/src/lib.rs)s:
4-byte handles into one process-wide interner, so names compare and hash
as integers and each distinct name is stored once. Interning checks a
64 KiB cache owned by the calling thread before a sharded table (128
shards with read-write locks), because a few names (`self`, `T`, `new`)
dominate and would otherwise make every thread queue on the same shard.
Resolving a symbol to its string takes no lock. Keywords, primitive types
and common names are interned first, so `kw::Fn` and `sym::u8` are
constants.

IRs will live in [`Arena<T>`](../crates/flux-arena/src/typed.rs)s and refer
to each other by `Idx<T>`, a typed 32-bit index: no lifetimes, half the
size of a pointer, and still valid after the arena moves to another
thread or is cached. `DroplessArena` bump-allocates strings, byte strings
and other `Copy` data that is freed all at once.

The two crates hold all of flux's `unsafe` code. Every `unsafe` block
states why it is sound (enforced by clippy), and `just miri` runs their
tests under Miri, which checks for undefined behavior including data
races.

## Performance

`just bench <dir>` parses every `.rs` file under a directory from memory
and reports each phase. On the local crates.io cache (57,451 files,
1.07 GiB) on an Apple M5 Max:

| Phase | ns/token | Share |
| --- | --- | --- |
| Lex | 5.9 | 33% |
| Build parser input | 1.2 | 7% |
| Parse | 5.2 | 29% |
| Build tree | 5.5 | 31% |

That is about 210 MiB/s on one thread and 2.9 GiB/s on 18 (files are
independent). Rowan trees took 24.4 ns/token to build, about 100 MiB/s
overall. Trees take 5.3 times the size of the source, text included,
mostly for nodes (0.69 per token, trivia included).

Interning all 52.6M identifiers and lifetimes (938K distinct) takes
12 ns/name on one thread, and 74 ms in total on 18.

Building `core`'s part of the front end (parsing, item tree, constants,
signatures) takes about 1.2 ms, once per process. A compilation in a
process that has built it, such as the test suites, starts from it:
copying its item tree takes about 5 µs and its signatures 9 µs, so
lowering and type-checking an empty program takes about 35 µs instead
of 1.1 ms. A single `flux build` still builds `core` once.

## Testing

| Test | Checks |
| --- | --- |
| `lexer.rs` unit tests | Token boundaries for each literal and punctuation form |
| `tests/parser.rs` `ok_fixtures`, `err_fixtures` | Snapshot trees in `tests/data/parser`; `ok/` has no errors, `err/` has some |
| `tests/parser.rs` `prefixes_parse_losslessly` | No panics, well-formed trees and exact round-trips on every prefix of every fixture |
| `tests/parser.rs` `workspace_sources_parse_without_errors` | flux parses its own sources |
| `tests/corpus.rs` (opt-in) | No panics or malformed trees over a directory, with an error summary |
| `flux-arena`, `flux-intern` unit tests | Allocation, alignment, threads; also run under Miri |
| `flux-hir` `tests/hir.rs` | Snapshot HIR for `tests/data/{ok,err}`; lowering never panics on any parser fixture prefix |
| `flux-ty` `tests/ty.rs` | Snapshot types and diagnostics for `tests/data/{ok,err}`; `main` rules; `core` is clean; signatures resolved after `core`'s shared ones match resolving everything; never panics |
| `flux-mir` `tests/mir.rs` | Snapshot MIR and initialization errors for `tests/data/{ok,err}` |
| `flux-cli` `tests/run.rs` | Every program in `tests/run`, built with rustc and with flux (with and without `-O`), produces the same output and exit status |
