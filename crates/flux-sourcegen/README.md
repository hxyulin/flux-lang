# flux-sourcegen

Generates the typed AST of [flux-syntax](../flux-syntax/README.md),
`src/ast/generated.rs`, from the grammar in `flux-syntax/flux.ungram`.

```sh
cargo run -p flux-sourcegen   # rewrite the generated file
cargo test -p flux-sourcegen  # check it is fresh, and that trees match the grammar
```

It does not depend on flux-syntax, so it still runs when a grammar change
leaves flux-syntax unable to compile; only its tests do.
