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
`MACRO_EXPR`, `MACRO_TYPE`, `MACRO_PAT`; see [Macros](#macros) for their
expansions) so that, for example, a `let`
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
  struct's, trait's, constant's and `impl` block's name, generic and
  lifetime parameters, bounds and signature, with types, lifetimes and traits as written
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

## Macros

Macro calls are expanded where HIR meets them
([`macros`](../crates/flux-hir/src/macros/mod.rs)): the item tree
expands calls in item position as it collects a module's items, in
textual order, and body lowering those in statement, expression and
pattern position; calls in type position are expanded before the types
around them are lowered (`expand_types`). Expansion works on tokens
([`tt`](../crates/flux-hir/src/macros/tt.rs)): raw tokens with where
each came from, glued into Rust's (`=>`, `::`) where matching compares
them. `macro_rules!` rules are matched as rustc's `mbe::macro_parser`
does ([`mbe`](../crates/flux-hir/src/macros/mbe.rs)): the matcher is
flattened into positions, every way of matching is followed a token at
a time, and a fragment (`$e:expr`) is read by the parser
(`parser::fragment_len`) on one `Input` of the call's tokens, so a
call's cost is linear in its tokens. Macros the compiler implements
(`assert!`, `format_args!`, `cfg!`, ...) are `core`'s
`#[rustc_builtin_macro]` declarations, expanded by
[`builtin`](../crates/flux-hir/src/macros/builtin.rs). `format_args!`
expands as rustc's lowering does: `Arguments::new_v1(&[pieces], &match
(&a, &b) { args => [rt::Argument::new_display(args.0), ..] })`, each
argument evaluated once and borrowed, one `rt::Argument` per argument
and trait (a width or precision from an argument is
`Argument::from_usize`), and `new_v1_formatted` with an array of
`rt::Placeholder`s (the argument, flags and fill in rustc's bit
layout, width and precision) where a placeholder has options or the
arguments are not in order. Each constructor call is at its argument,
so an unmet bound is reported there, and errors inside the string
point into it, through escapes (`source_offsets`). `std`'s printing
macros are `macro_rules!` in `core` that call `core::io`'s `_print`;
resolution finds them by name or through `std::`, never `core::`.

An expansion's tokens are written out as text and parsed as what the
call's position wants (`parser::Fragment`: items, statements, an
expression, ...) into a tree of its own, whose ranges are not its text's
but where its tokens came from (`SyntaxTree::set_origins`): each token
has a range in the file and a *group*, the file's own tokens or one
macro definition's in that file, and a node is where its tokens are if
they are all of one group and in order, and at the call otherwise. A
call's arguments keep their ranges, a local macro's definition is
pointed into, and everything `core`'s macros make is at the call, as in
rustc's diagnostics. So the HIR made from an expansion has ranges in
the file like any other, and nothing after it knows macros exist.
Items an expansion in item position makes are in a file of their own
in the `SourceDb` (`SourceFile::expansion_of`), whose diagnostics are
reported in the call's file.

Hygiene is rustc's for `macro_rules!`: each expansion gives the tokens
its definition wrote a fresh context (a number on each token of the
tree), and locals and labels are looked up by name and context
(`lower::scopes::Ident`); a context made by a macro defined in the body
being lowered also sees the bindings in scope at the definition, as
rustc's ribs do. Items are found by name alone. A macro is in textual
scope after its definition (`MacroScope`, a persistent list with jump
pointers, so finding a name is logarithmic); functions and constants
record theirs for lowering their bodies.

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

An `impl Trait` in a function's parameters is lowered, in the item
tree, to a type parameter of its own after the function's others
(`GenericParam::synthetic`, `TypeRef::ImplParam`), which generic
arguments cannot give. One in its return type is an *opaque type*
(`TyKind::Opaque`): the function, its index among the function's
(`Function::opaques`), and the function's generic parameters as its
arguments, since edition 2024 captures them all. Its bounds
(`Signatures::opaques`, with the opaque type as an extra parameter)
are all that is known of it, as a parameter's are: the solver's item
bounds (`traits::alias_bounds`) and normalization give its methods and
associated types. Inferring the function's own body, each of its
opaque types is a variable instead (and so is the type a call of
itself returns), whose bounds are obligations; once the body is
inferred, the variable's value is the *hidden type*
(`Signatures::hidden`). MIR building gives the body's return place the
hidden types, and monomorphization (`traits::instance_ty`) and the MIR
validator reveal every opaque type, so code generation sees only the
hidden ones.

A type parameter is `Sized` unless it is a trait's `Self` or bounded
`?Sized` (`GenericParam::maybe_unsized`, from the item tree): rather
than an implicit predicate per parameter, as rustc has, each item knows
which of its parameters may be unsized (`unsized_params`), and a
`Sized` goal holds of every type but slices, `str`, trait objects and
those parameters, unless a bound says so. Calls check their arguments
for the callee's sized parameters, and matching an `impl` block's
header rejects unsized arguments for its sized ones.

A trait object (`TyKind::Dynamic`, [`objects`](../crates/flux-ty/src/objects.rs))
is its trait, the trait's arguments and the values of every associated
type of the trait and its supertraits, in one order (`dyn_assoc`), so
`dyn Iterator<Item = u32>` is one type however it is written. Whether a
trait can be one (rustc's dyn compatibility, E0038) and its supertraits
are read from the item tree alone, as the types that name it are
resolved. Its bounds are its trait elaborated with `Self` the object
(`traits::dyn_bounds`), which the solver, method probing and calls of
`Fn`-family objects find as they find an opaque type's, and its
associated types normalize to what it gives (`dyn_projections`).
`coerce` unsizes a reference or pointer to a value whose type must then
implement the trait, with its associated types, and be sized
(obligations), and upcasts one to an object whose trait has the target's
as a supertrait; casts try the same coercion. A trait object has one
region slot, its lifetime bound, covariant: as written, or rustc's
default, the reference it is behind (`'static` elsewhere); elision
counts each parameter's distinct lifetimes, as rustc does, so `&dyn
Tr` has one. Borrow checking an unsizing makes what the value holds
(its slots, and its parameters by type tests) outlive the bound, and
a mutable reborrow of a trait object may shorten it, as rustc's
unsizing coercion may.

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
value needs its own length is a cycle. A length written as another
expression (`[u8; N + 1]`) is an *anonymous constant*, as in rustc:
the item tree collects one per such expression in each item and body
(`ItemTree::anon_consts`, by file and node, the file being a macro
expansion's for the items it makes; `Const::anon`), of type
`usize`, lowered and evaluated like any constant; rustc's
`min_const_generics` rules (no generic parameters, no generic `Self`)
are checked as it is collected.

Constants in types are types too: a length or const generic argument
is a `TyKind::Value` (a value and its type), a const parameter is a
`TyKind::Param` like a type parameter (the item's `generic_consts`
says which are const, and their types), and one to infer a variable.
So an array is `TyKind::Array(T, len)` with `len` such a type, generic
arguments stay one list, and substitution, unification (`[i32; ?N]`
with `[i32; 3]`), `impl` matching (`impl<T, const N: usize> Tr for [T;
N]`) and monomorphization treat them as they treat types. A generic
argument is resolved for its parameter (`resolve_generic_arg`): a path
is a type if it names one, else a constant, and a `{ expr }` block an
anonymous constant whose type, its parameter's, is known only once
the path is resolved (`Signatures::arg_tys`), so it is evaluated in
the next round. A const parameter used as a value is MIR's
`Constant::TyConst`, which substitution makes an integer, `bool` or
`char`.

Where the type an expression must have is known before it is inferred
(a `let` with a type, an argument, a field, a return value), it is
noted, and a call or struct expression unifies its result with it first,
as rustc does: `let x: u8 = id(1i32)` is a mismatch at `1i32`. The
expectation flows into the parts that are themselves coercion sites, as
in rustc: a tuple's elements, an array's, the operand of `&`, an `if`'s
or `match`'s branches and a `loop`'s `break` values, each coerced to its
part of the type wanted, which the tuple, branch or loop then has (so
`[(1, &a)]` with `&a` unsized is an array of tuples of slices, built as
one). Branches without a type wanted meet at rustc's least upper bound:
each is coerced to the type of those before, or those before to its.

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

A method call `x.m()` looks through what `x` dereferences to for the
type with a method `m`, and records how the receiver is passed:
dereferenced so many times, then borrowed for `&self` and `&mut self` or
passed by value. A field access and an index dereference the same way.
Each step of this *autoderef* (`deref_step`) goes through a reference or,
for a type that may implement `core::ops::Deref` (`#[lang = "deref"]`),
to its `Target`, as rustc's does; the overloaded steps are recorded by
their index among the expression's dereferences
(`InferenceResult::overloaded_derefs`, `OverloadedDeref`), as `Deref`
calls until the place turns out to be used mutably (assigned, borrowed
`&mut`, the receiver of a `&mut self` method), when they become
`DerefMut` ones where the type has it, as rustc's
`convert_place_derefs_to_mutable` does; `*w` is one step. Where a `&mut
T` is used as a reference (an argument, or a `let` with a type), it is
reborrowed rather than moved, and may become a `&T`, as in rustc; a
block ending with one is reborrowed at its tail. A reference to
something that dereferences to the type expected (`&&T`, `&mut &T`,
`&Wrapper<T>`) is dereferenced until it is the reference expected, then
reborrowed (`&**r`): rustc's deref coercion, whose overloaded steps are
`coercion_derefs`.

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

Types have no lifetimes: bodies are inferred as if every reference had
the same one, as rustc's type checking erases them, and a `Ty` stays a
handle that compares by identity. What the borrow checker needs of
lifetimes is kept beside signatures ([`regions`](../crates/flux-ty/src/regions.rs)):
for each type in a function's, struct's, `impl` block's or constant's
signature, the lifetime of each of its *region slots*, in a fixed
order (a reference's own, then its pointee's; a struct's lifetime
arguments, then its type arguments'; a closure's captures'; a generic
parameter and an associated type have none, an instance gets its
arguments' at their places). A signature's lifetimes are its
parameters (its owner's, then its own, then one per lifetime elided
in a function's inputs or an `impl` block's header), `'static` or, in
bodies, ones to infer. Resolving them applies rustc's elision rules
and reports its errors (E0106, E0261, E0107, ...); then each struct's
parameters get their variances, and what its fields imply its
parameters outlive (`&'a T` that `T: 'a`, rustc's inferred outlives
requirements), and each function what its bounds and parameter types
say its lifetimes outlive. A function's and an `impl` block's bounds
record the slots of the types they are on and of what they say
associated types are (`I: Iterator<Item = &'a T>`), in the order of
`fn_predicates` and `fn_projections`. A trait's `impl` block is chosen
by types alone, as in rustc; what its header says of lifetimes
(`impl<'a> PartialEq for Token<'a>`, `impl Tr for &'static S`) is the
borrow checker's to enforce, below.

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
  checks these even in release builds; a failed assertion panics.
- `a[i]` copies `i` into a temporary, asserts that it is less than the
  length (an array's, or a slice's from the pointer it is behind),
  and is the place `a[_n]`; a move out of it is E0508, and borrowck
  takes any two elements of one array to overlap, as rustc does.
  Other indexing (`&a[1..3]`, `grid[(r, c)]`) is a call of
  `Index::index` or `IndexMut::index_mut`, which type checking chose
  by whether the place is used mutably, on a borrow of the base
  (unsized if type checking found the method on the slice an array
  unsizes to), and is the place `(*_r)` the call returns. An
  overloaded dereference is the same: `Deref::deref(&place)` or
  `DerefMut::deref_mut(&mut place)`, and the place `(*_r)`; its
  temporary (`LocalKind::Overloaded`) lets the borrow checker name what
  was dereferenced or indexed where rustc does ("cannot move out of
  dereference of `W`", "cannot borrow data in dereference of `W` as
  mutable" where it has no `DerefMut`).
  `[x; N]` is one rvalue, `Repeat`; `len` reads the length (`Len`), as
  a borrow of its receiver; an unsizing coercion is an `Unsize`
  rvalue. Drop glue drops an array's elements in a loop. Arrays'
  traits are `core::array`'s `impl<T, const N: usize>` blocks, like
  any others; `core`'s `Drop` impls (`array::IntoIter`'s) are built
  whether or not a call names them, as drop glue may call them.
  `core`'s `#[rustc_intrinsic]` functions (`core::intrinsics`: `read`,
  `write`, `uninit`, `is_copy`, `subslice`, ...) are what the borrow
  checker sees by their signatures; `mono` replaces their calls with
  MIR once types are known (a copy through a reference, nothing for
  `uninit`'s memory to be written, a constant for `is_copy`, a
  `Subslice` rvalue), so their bodies never run.
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
  it is (`Constant::FnItem`), so a call of one is a direct call. Coerced
  to a pointer, it is a constant (`Constant::Fn`), and a call through a
  pointer is a `CallIndirect` terminator.
- MIR is well-typed: every assignment's value has its place's type, and
  every call's arguments and result its callee's types, coercions being
  explicit rvalues or reborrows. Code generation trusts the types of
  places, so [`validate`](../crates/flux-mir/src/validate.rs) checks this
  of every body in debug builds, and of every corpus file that builds
  in `just corpus`.
- A static is a place behind a temporary that holds its address, as in
  rustc's MIR: `&T`, or `*mut T` for a `static mut`, so `X.a` is
  `(*_3).a`. Borrow checking sees it behind a pointer, and names it as
  rustc does ("cannot assign to immutable static item `X`"); constant
  evaluation reads it as its initial value.
- `&` of a constant expression (`&5`, `&[1, 2]`) is *promoted*, as in
  rustc: `Constant::Promoted`, a reference to a value in the body's
  `promoted` list, which lives forever. Constant evaluation gives a
  shared reference to a copy of what it borrows. A borrowed temporary
  dies (`StorageDead`) where its scope ends.
- `x?` moves `x` into a temporary, as rustc's `Try::branch(x)` takes
  it, and tests its variant: the `Some` or `Ok` value is its result;
  otherwise `_0` becomes `None`, or `Err` with the error passed through
  `From::from` (unless the types are the same), and the function
  returns, dropping what is in scope as `return` does.

Only reachable code is built: every function of the program, and the
functions of `core` that something calls or names as a value (a
function item passed on, `.all(u8::is_ascii_digit)`, whose shim calls
it once monomorphized, or a pointer). A generic function is built
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
`#[lang = "panic"]` function (and `panic_display` of a `&str`) prints
its message and aborts at run time; evaluation reports a call of it as
rustc's "evaluation panicked", with its message, and never runs its
body, which `#[rustc_do_not_const_check]` exempts from the rules of
`const fn`s. Uses of a
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
rustc's non-lexical lifetimes. Each borrow is a *loan*, and
[`regions`](../crates/flux-mir/src/borrowck/regions.rs) infers its
region as rustc's NLL does: a region variable for every slot of every
local's type, each borrow, and each lifetime of each call's callee
(with fresh ones for its generic arguments' slots); outlives
constraints from subtyping at assignments, aggregates, calls and
returns (with each type's variance: `&mut T` invariant in `T`, a
struct's parameters as its fields use them), from reborrows (`&*r`
needs `r`'s region, up to the first shared reference) and from the
callee's bounds and implied requirements; and values over the body's
points (each statement and terminator), where each local is live (may
be read or dropped later). The constraint graph's strongly connected
components share values, computed bottom-up as interval sets. The
function's lifetime parameters and `'static` are *universal*: live
everywhere, with an end marker each; one whose value has another's
marker must outlive it, which the signature must say ("lifetime may
not live long enough", at the constraint rustc would blame, once per
lifetime; E0521 where a parameter's must be `'static` for a call).
A call's trait bounds (`Self: Tr` of a trait's method, then the
callee's own) are proven as rustc's borrow checker proves them: the
`impl` block chosen by types is matched to the bound's slots, which
equates a lifetime or type its header names twice, and with `'static`
one it names so; its lifetime bounds become constraints, and its own
bounds are proven in turn; so is `Copy` where a struct or enum is
copied out of a place. What the callee's bounds say associated
types are must be what they normalize to, lifetimes included, and in
the body, such an associated type has the bound's lifetimes. Callee
signatures' associated types are normalized with their `impl` blocks'
lifetimes; where a signature cannot be matched to the instance (an
associated type no block decides, a function pointer), what the call
returns is outlived by everything it is given. A loan is in scope from
where it is made at every point its region holds that is reachable
without leaving it or passing a write of the place it borrows (or a
place containing it), found per loan by a walk over runs of points.
Each read, move, assignment and borrow of a place is checked against
the loans in scope at its point of places that contain it or are
contained in it, so borrows of different fields never conflict, giving
rustc's E0499, E0502, E0503, E0505 and E0506. A method receiver's
`&mut` is two-phase: shared while the temporary holding it is live,
until the call, so `s.set(s.get())` is accepted, and mutable after;
as in rustc, one unsized to a slice for the call is not two-phase. A loan in scope where its local dies (a binding's or
borrowed temporary's `StorageDead`, a temporary's drop, or the
function's return for parameters and temporaries) is E0597 or E0716,
and E0515 when the constraint that makes it outlive the function is
the return value's. The pass also reports `&mut` borrows of non-`mut`
bindings and anything written or mutably borrowed behind a `&`
(E0596, E0594), and moves out of a reference (E0507) or of a field of
a struct with a `Drop` impl (E0509), which moves nothing, so the value
is not also "moved" for `init`. A closure's body is checked before its
creator's: each lifetime of what it captures, its parameters and its
result is a universal region of its own, and how they must relate
becomes its *requirements*, which its creator adds where it makes the
closure (between what it captures) and where it calls it. A closure
passed to a generic function is called there as the callee's bounds
say: proving a `Fn`-family bound on it (the call's own, or an `impl`
block's, `F: FnMut(I::Item) -> B` of `Map<I, F>`'s) relates what it
requires of its parameters' lifetimes to the bound's parameter types,
whose own references (`&T` in `F: Fn(&T)`) are higher-ranked, so
nothing else may outlive them or they it; and its `Output` normalizes
to new regions that what its body returns outlives, which binds `B`
for normalizing `<Map<I, F> as Iterator>::Item`. Where no bound says,
what it returns is outlived by everything the call is given. In a closure's
body, what it captures is a field of `self`, through the reference it
captured unless by value, and is named and checked as the captured
variable (E0594 and E0596 for an `Fn` closure's, E0507 for moves out
of an `FnMut` closure's). A constant's value is `'static`, while the
temporaries in it live forever. An opaque type's region slots are the
lifetimes it captures, then its arguments'; it is invariant in all of
them, and a caller's value of it keeps them live. In its function's
body, its hidden type's slots are new variables, each of which must
outlive one of the lifetimes it captures (`'static` if none), the
least that what flows into it allows, or all of its `+ 'a` bounds, as
rustc's member constraints say; one of the function's lifetimes it
does not capture flowing into it is E0700, and what its bounds say
associated types are (`Item = &'a T`) bounds the hidden type's (or,
where that cannot be told, all of its lifetimes). A capture's loan
escaping that way is E0373. A trait's `impl` block's methods are
checked against the trait's signatures the same way, with the trait
method's lifetimes universal: they may ask no more of them.

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
- A local that is borrowed lives in a stack slot instead, and so does
  whatever a pointer points to: reading or writing a place there loads
  or stores each leaf at its offset. Values in memory are laid out as
  rustc lays them out ([`layout`](../crates/flux-mir/src/layout.rs),
  rustc's `rustc_abi::layout` for aarch64), so `size_of` and memory
  written through raw pointers agree with rustc: a struct's, tuple's
  or closure's fields reordered by alignment, the field with the
  largest niche (its invalid values: a reference's null, a `bool`'s 2
  to 255) first in its group, unless `#[repr(C)]`, and a tuple's last
  element left last; an enum tagged, its tag the smallest integer that
  holds its discriminants (or its `repr`'s), widened to the alignment
  of the variants' first fields, or niche-filling, its largest
  variant's fields with the others told apart by values of their
  largest niche (`Option<&T>` is one pointer), whichever is smaller;
  an enum with one variant that may hold a value laid out as a struct.
  `NonNull` (`#[lang = "non_null"]`) has a null niche and `MaybeUninit`
  none.
- A value that contains an array is indexed at run time, so it lives
  in memory whatever happens to it: its locals are stack slots, its
  value is the address of one, copied with `memmove` where it is
  assigned, it is passed to a function as that address (the callee
  copies it) and returned through `sret` memory laid out as the value
  is. An element is at its index times its size. A reference to a
  slice is two leaves, the address of the first element and the
  length, which an unsizing coercion adds to a reference to an array
  and `len` and bounds checks read.
- An enum's leaves are its discriminant, of its `repr` type, then
  every variant's fields in turn; only the variant it holds has
  meaningful leaves. In memory its variants share their bytes, so a
  store stores the fields of the variant the discriminant says (through
  a `switch` where several have fields), then its tag; a load decodes
  the tag, extending a direct one, and comparing a niche with the values
  the other variants have in it. `size_of::<T>()` and `align_of::<T>()`
  (`core`'s intrinsics) are constants from the layout once `T` is
  known, also in constant evaluation; for a type flux lays out
  otherwise than rustc (`&CStr`, one pointer here), they are not
  supported yet.
- Aggregates are passed as one argument per leaf. keel functions return
  at most one value, so a function returning two or more leaves returns
  them through an `sret` pointer to a C struct of the leaves in the
  caller's frame; keel returns small ones in registers.
- `bool` is an `i8` holding 0 or 1, a `char` an `i32` holding its code
  point, compared and extended as unsigned; `()` and `!` have no value and
  disappear from signatures. Narrow integers are extended by signedness
  at calls, as Apple's arm64 ABI requires of C callers.
- keel takes shift amounts modulo the width, which is exactly Rust's
  masked shift; the amount is converted to the shifted value's type.
- Pointers, including `&CStr`, are keel `ptr`s. Each distinct C string
  becomes a module-local, read-only data object ending in a NUL byte,
  and each static a data object holding its evaluated initial value,
  laid out as in memory, writable only for a `static mut`. A reference
  in a constant's value, and a promoted constant, points to a read-only
  data object of what it borrows, shared by identical ones; inside a
  static's data, the linker fills in its address. A string or byte
  string literal is such a promoted constant, its bytes, and a `&str`
  two leaves as a reference to a slice is. A cast between pointers is
  no instruction (one to a thin pointer drops the length), and one
  between a pointer and an integer `PtrToInt` or `IntToPtr` with the
  integer extended or truncated to 64 bits.
- A function pointer is a keel `ptr`, a function's `symbol_addr`, and
  a call through one keel's `call_indirect` with C's convention. Since
  keel allows `cc(fast)` for direct calls only, monomorphization
  collects the instances whose address is taken, and those use C's
  convention everywhere.
- A pointer to a trait object is two leaves: the address of the
  value and that of its type's vtable
  ([`vtable`](../crates/flux-mir/src/vtable.rs)), a read-only data
  object of the drop function (`core::ptr::drop_in_place::<T>` where
  dropping a `T` runs code, else empty), size and alignment, the
  addresses of its supertraits' vtables, then each method of the trait
  and its supertraits, the type's `impl` block's or the provided one
  (empty for one with `where Self: Sized`). Unsizing to a trait object
  emits the vtable, once per type and object; upcasting loads the
  supertrait's from the object's. A call of a trait's method whose
  `Self` is a trait object is a *virtual call*: it loads the method
  from its slot and calls it through the pointer, passing the receiver
  as the value's address alone, as the method's `&self` takes it.
  `mono` collects the methods of each vtable a body makes as instances
  whose address is taken, which use C's convention, and does not
  collect virtual calls' callees; a body that makes a trait object
  reaches every method of its trait and supertraits, so that `core`'s
  are type-checked and built.
- Calls to C-variadic functions use a variadic keel signature, which
  gives the platform's convention (on Apple arm64, variadic arguments go
  on the stack).
- Rust functions get mangled symbols from their whole path
  (`_ZN3fib3fibE`, `_ZN4core3num3i323absE` for `core::num::i32::abs`), so
  they cannot collide with C names or each other. The C `main` is a wrapper that calls the Rust
  `main`, then `core::io::flush_stdout` (std's runtime writes out
  what stdout holds as `main` returns), and returns 0.
- A `#[track_caller]` function takes a pointer to a
  `core::panic::Location` after its parameters (unless its address is
  taken: a pointer's signature has no room for it), which every call
  of it passes: the caller's own, if the caller takes one, else a
  read-only data object of the call's file, line and column, counted
  in characters as rustc counts them. Which function a call reaches is
  known only once it is resolved for its instance, so code generation
  decides, as rustc's does. A method call is located at its name
  (`Body::fn_spans`), an overloaded index at its `[`, other calls at
  their start. The `caller_location` intrinsic, which
  `Location::caller` calls, is that pointer. A failed MIR assertion
  calls `core::panicking`'s function for its message
  (`panic_bounds_check` with the index and length,
  `panic_const_div_by_zero`, ...), which the collector and type
  checking always reach (`flux_ty::core_roots`), at the assertion.

### Formatting

[`core::fmt`](../library/core/src/fmt.rs) is rustc's design: a
`Formatter` holds its options (`flags` with the fill character in its
low bits, `width` and `precision`, as rustc's) and a `&mut dyn Write`,
and the builders take `&dyn Debug`. An `rt::Argument` is a value's address and
the function that formats a `T` there with the trait the placeholder
names (`display_at::<T>`). `fmt::write` walks the pieces and
placeholders, giving each value a formatter with its options, and the
`{:#?}` builders write nested values through a `Formatter` over a
`PadAdapter`, which indents. [`core::io`](../library/core/src/io.rs)
writes to file descriptors 1 and 2 through C's `write`, buffering
stdout by lines in a `static mut` as std's `LineWriter` does, and
prints panics as std's default hook does, with the thread id from
`pthread_threadid_np`. Once `alloc` lands, `String` can implement
`Write`, which `format!` and `to_string` need.

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
the arguments `C` holds (`FileMir::instance_body`). In its body, a
place outside it is found in what it captures with the dereferences
that use it adds: `|| r.get()` captures `*r`, so its receiver is the
capture itself, never `r`, and the same holds for autoderef in
fields, indices and `len`, reborrows, and patterns, which resolve a
scrutinee the closure captures only parts of as they go into it. A
closure is an aggregate of what it captures, so code generation needs
nothing new for it. Code type checking rules out (a local neither
declared nor captured, a call of a value that is not callable) is
lowered to `unreachable` and recorded in `Body::bugs`, and so is a
local whose type is an error (`Ty::has_error`), which only a reported
error may make; monomorphization likewise refuses an instance whose
generic arguments are errors or parameters. Bodies without
errors of their own reach it where others have some, such as a call
of what a failed import should have brought, and those programs are
never compiled; in a program without errors it is a bug in flux, on
which `build_file` panics in debug builds and reports an error in
release builds, and the corpus test fails. Function items and pointers get shims that call them, and a
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
| `flux-cli` `tests/run.rs` | Every program in `tests/run`, built with rustc and with flux (with and without `-O`), produces the same output, panic messages and exit status |
