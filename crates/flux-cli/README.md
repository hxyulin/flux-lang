# flux-cli

The `flux` command for [flux](../../README.md), an experimental compiler for
a subset of Rust.

```sh
flux lex file.rs      # tokens, trivia included
flux parse file.rs    # syntax tree and errors
flux check a.rs b.rs  # errors only; exits with 1 if there are any
```
