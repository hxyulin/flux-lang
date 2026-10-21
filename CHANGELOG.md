# Changelog

## Unreleased

### Syntax

- Add a lexer for Rust edition 2024 tokens: nested comments, raw
  identifiers, lifetimes, char, byte, string, raw string, byte string and C
  string literals, and integer and float literals with suffixes.
- Add an error-recovering parser for items, generics, types, patterns,
  expressions and statements in the [supported subset](docs/LANGUAGE.md),
  producing lossless syntax trees.
- Add `flux lex`, `flux parse` and `flux check`.
- Add a typed AST (`flux_syntax::ast`) generated from `flux.ungram` by the
  new `flux-sourcegen` crate, whose tests check that parsed trees match
  the grammar.
- Add `flux-hir`: the item tree and HIR lowering of function bodies,
  with names resolved, and `flux hir` to print it.
- Give macro calls a node kind per position: `MACRO_CALL` (items),
  `MACRO_EXPR`, `MACRO_TYPE` and `MACRO_PAT`.

### Performance

- Build flat syntax trees (preorder node arrays over the lexer's token
  arrays) instead of rowan green trees: 5.5 ns/token to build instead of
  24.4, and about 210 MiB/s for whole-file parsing on one thread instead
  of 100. The trees are identical to the rowan ones across the corpus.

- Lex bytes instead of characters, skipping comments and strings with
  `memchr` (8.9 to 6.0 ns/token), and shrink parser events from 32 to 8
  bytes (5.5 to 4.6 ns/token).
- Add `flux-intern`: a global, sharded `Symbol` interner with per-thread
  caches, lock-free resolution and predefined keywords and names.
- Add `flux-arena`: `DroplessArena` for bump allocation, and `Arena<T>`,
  `Idx<T>` and `ArenaMap` for index-based IRs.
- Add `just bench` and `just miri`.
