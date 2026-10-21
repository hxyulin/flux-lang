# flux-syntax

Lexer, error-recovering parser and lossless syntax trees for
[flux](../../README.md), an experimental compiler for a subset of Rust.

```rust
let parse = flux_syntax::parse_source_file("fn main() {}");
assert!(parse.errors().is_empty());
println!("{}", parse.debug_dump());
```

Parsing always produces a tree whose text equals the input, along with any
errors. Trees are flat arrays that own their text, and are `Send + Sync`.
`flux_syntax::ast` is a typed view of the tree, generated from
[`flux.ungram`](flux.ungram) by [flux-sourcegen](../flux-sourcegen). See
the [architecture guide](../../docs/ARCHITECTURE.md) for how the stages fit
together.
