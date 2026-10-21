# Repository guide for coding assistants

flux is an experimental compiler for a subset of Rust (edition 2024). Use
[CONTRIBUTING.md](CONTRIBUTING.md) for setup and validation;
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) explains the pipeline.

## Sources

- `crates/flux-syntax/`: syntax kinds, lexer, parser, tree building, and the
  typed AST (`src/ast/`), generated from `flux.ungram`.
- `crates/flux-sourcegen/`: generates `src/ast/generated.rs`; its tests
  check that parsed trees match `flux.ungram`.
- `crates/flux-intern/`: global `Symbol` interner and predefined symbols.
- `crates/flux-arena/`: `DroplessArena` and typed `Arena<T>`/`Idx<T>`.
- `crates/flux-cli/`: the `flux` binary (`lex`, `parse`, `check`).
- `crates/flux-syntax/tests/data/parser/`: parser fixtures. `ok/` must parse
  cleanly, `err/` must report errors; each `.rs` has a `.rast` snapshot.
- `examples/`: small programs for trying the CLI.
- Do not commit `target/` or `out/`.

## Conventions

- Rust stable, edition 2024, selected by `rust-toolchain.toml`. No MSRV.
- The parser reads token kinds only, never source text, and never panics on
  any input. The lexer and parser always make progress and always produce a
  tree whose text equals the input.
- flux accepts only valid Rust. When the parser accepts something rustc
  rejects (or the reverse), that is a bug unless docs/LANGUAGE.md lists it.
- Add grammar in the module for its construct (`items.rs`, `expressions.rs`,
  ...). New node kinds go in `syntax_kind.rs` under the matching section,
  and in `flux.ungram`; then run `just sourcegen`. Never edit
  `ast/generated.rs` by hand; accessors the generator cannot produce go in
  `ast/ext.rs`.
- Type checking does not happen on the AST: the AST is a typed view of
  syntax, and semantic analysis works on HIR lowered from it.
- Performance is a design goal: prefer flat arrays, 32-bit indices and
  interned symbols to pointers and owned strings, and measure claims with
  `just bench` (run it more than once; it varies by a few percent).
- `unsafe` lives in `flux-arena` and `flux-intern` only. Each block needs a
  `// SAFETY:` comment; changes there must pass `just miri`.
- Keep error messages short and lowercase, naming what was expected:
  "expected a type", "expected `;`".

## Verification

Run checks appropriate to the change before finishing:

- `just check`: format, clippy, tests, and `flux check` over the workspace.
- Parser changes: add or extend a fixture in `tests/data/parser/{ok,err}`,
  run `just snapshots`, and read the `.rast` diff to confirm the new tree.
  `cargo test -p flux-sourcegen` then checks the tree against
  `flux.ungram`.
- Larger grammar changes: `just corpus ~/.cargo/registry/src` and compare the
  error summary before and after; panics and lossy trees fail it.

Report the behavior changed, the validation run and remaining limitations.
Do not commit or push unless asked.
