# Architecture

flux is a Cargo workspace:

| Crate | Role |
| --- | --- |
| [`flux-cli`](../crates/flux-cli/src/main.rs) | The `flux` binary: files, arguments, diagnostics, exit codes |
| [`flux-syntax`](../crates/flux-syntax/src/lib.rs) | Source text to lossless syntax trees, and the typed AST over them |
| [`flux-sourcegen`](../crates/flux-sourcegen/src/lib.rs) | Generates the typed AST from [`flux.ungram`](../crates/flux-syntax/flux.ungram) |
| [`flux-hir`](../crates/flux-hir/src/lib.rs) | Item tree and HIR: function bodies with names resolved |
| [`flux-ty`](../crates/flux-ty/src/lib.rs) | Type inference and checking |
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
   and `match`, `?` to a `match`, `if let` chains to `match`), keeping a
   map back to AST nodes for diagnostics.
3. **Type inference and trait solving** on HIR bodies, one body at a time,
   given the item tree's signatures.
4. **MIR** for borrow checking and lowering to keel.

## HIR

[`flux-hir`](../crates/flux-hir/src/lib.rs) builds two things from a
file's AST:

- The **item tree**: every function's name and signature, with types as
  written (`TypeRef::Path(u8)`), but no bodies. Everything outside a
  body depends only on it.
- A **body** per function, lowered into arenas of expressions, patterns
  and bindings addressed by `Idx`. Names are resolved while lowering,
  through a stack of scopes and then the item tree, so an expression
  `x` becomes `Res::Local(binding)` or `Res::Function(id)`. `break` and
  `continue` record the loop they leave. Parentheses disappear, and
  `while c { body }` becomes `loop { if c { body } else { break } }`, as
  in rustc. A source map gives every node's text range for diagnostics.

Bodies depend only on the item tree, so they lower independently (and
in parallel, later). Syntax outside the [supported subset](LANGUAGE.md)
becomes `Expr::Missing` with a "not supported yet" error; flux never
silently ignores code, and ignores only attributes that cannot change a
program's meaning (`allow`, `inline`, `doc`, ...).

## Types

[`flux-ty`](../crates/flux-ty/src/lib.rs) resolves every signature from
the item tree, then infers each body on its own against them. Inference
is unification over type variables:

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

It also makes the checks rustc makes before borrow checking: calling an
`unsafe` function needs an `unsafe` block, and assigning needs `mut`,
except for the first assignment of a `let x;` binding. Telling a first
assignment from a second (and use before initialization) needs control
flow, so that check moves to MIR.

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
| `flux-ty` `tests/ty.rs` | Snapshot types and diagnostics for `tests/data/{ok,err}`; `main` rules; never panics |
