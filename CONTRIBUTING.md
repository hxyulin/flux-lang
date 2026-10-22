# Contributing to flux

flux is an experimental compiler for a subset of Rust. Start with the
[architecture guide](docs/ARCHITECTURE.md) and the
[supported language](docs/LANGUAGE.md). Small fixes, fixtures and clear bug
reports are useful contributions.

## Set up

- Rust stable, with `rustfmt` and `clippy` (selected by `rust-toolchain.toml`).
- [just](https://github.com/casey/just) for the repository recipes.
- [actionlint](https://github.com/rhysd/actionlint) for workflow edits; CI pins 1.7.12.

```sh
git clone https://github.com/hxyulin/flux-lang.git
cd flux-lang
cargo build
just check
```

`just` lists the available recipes.

## Find the right source

| Location | Purpose |
| --- | --- |
| `crates/flux-syntax/src/syntax_kind.rs` | Every token and node kind, and the `T![]` macro |
| `crates/flux-syntax/src/lexer.rs` | Text to tokens |
| `crates/flux-syntax/src/parser/` | Tokens to events; `grammar/` holds one module per construct |
| `crates/flux-syntax/src/build.rs` | Events and tokens to a syntax tree, placing trivia |
| `crates/flux-syntax/src/tree.rs` | `SyntaxTree`, `SyntaxNode`, `SyntaxToken` and navigation |
| `crates/flux-syntax/flux.ungram` | The shape of every node, from which the typed AST is generated |
| `crates/flux-syntax/src/ast/` | Typed AST: `generated.rs` (do not edit) and hand-written `ext.rs` |
| `crates/flux-sourcegen/` | AST generator; its tests check trees against the grammar |
| `crates/flux-cli/` | `flux` command-line driver |
| `crates/flux-syntax/tests/` | Snapshot, robustness, self-hosting and corpus tests |
| `crates/flux-syntax/examples/bench.rs` | Per-phase throughput benchmark |
| `crates/flux-hir/` | Item tree and HIR lowering: names resolved, `while` desugared |
| `crates/flux-ty/` | Type inference and checking, one body at a time |
| `crates/flux-mir/` | MIR: control-flow graphs with explicit runtime checks; initialization check |
| `crates/flux-codegen/` | MIR to keel IR, native objects and executables |
| `crates/flux-cli/tests/run.rs`, `tests/run/` | Programs built with rustc and flux; output and exit status must match |
| `crates/flux-intern/` | `Symbol` interning, predefined keywords and names |
| `crates/flux-arena/` | `DroplessArena`, `Arena<T>`, `Idx<T>`, `ArenaMap` |

## Validate a change

```sh
just check                 # formatting, clippy, tests, flux check on the workspace
just snapshots             # rewrite parser snapshots after an intended change
just sourcegen             # regenerate the typed AST from flux.ungram
just corpus <dir>          # parse a directory of Rust, report panics and common errors
just bench <dir> [threads] # per-phase throughput over a directory
just miri                  # run the crates with unsafe code under Miri (needs nightly)
just parse <file>          # print a syntax tree
just hir <file>            # print the HIR of a file
just mir <file>            # type-check a file and print its MIR
just run <file>            # compile a program with flux and run it
```

Every parser change needs a fixture. Put valid Rust in
`crates/flux-syntax/tests/data/parser/ok/` and invalid Rust in `err/`, run
`just snapshots`, and review the `.rast` diff: a snapshot passing only
proves the output did not change, not that it is right. The test suite also
parses every prefix of every fixture, so recovery bugs that panic or lose
text show up without dedicated tests.

For performance work, run `just bench` before and after on the same
corpus, at least twice: timings vary by several percent between runs.
Each compiler crate's `tests/scaling.rs` fails if a stage given four
times the input takes six times as long, and `just corpus` fails on any
file that takes too long for its size; extend them when you fix a
slowdown worse than linear, and check that the test fails without the
fix. Ignored scaling tests record known quadratic behavior.
Measuring inside the code under test (counters, timers) can distort it,
especially with many threads.

`unsafe` code belongs in `flux-arena` or `flux-intern` unless there is a
measured reason. Every `unsafe` block needs a `// SAFETY:` comment, and
changes to those crates must pass `just miri`.

A new node kind, or a change to what a node contains, also needs
`flux.ungram` updated and `just sourcegen` run. `cargo test -p
flux-sourcegen` fails if any tree in the fixtures or the workspace has a
child its rule does not mention, or if the generated code is stale.

For grammar work, `just corpus ~/.cargo/registry/src` parses every crate
you have downloaded. Compare its summary before and after a change; it
fails on any panic or malformed tree, or on a tree without errors that
does not match `flux.ungram`.

Programs in `tests/run` are compiled with rustc (`-C overflow-checks=off
-C panic=abort`) and with flux, with and without `-O`, and must produce
the same output, panic messages and exit status: `cargo test -p
flux-cli --test run`. Both are given the program's path relative to the
repository, which panic messages show.
This needs arm64 macOS, where flux can link executables so far. Write
programs so rustc cannot fold them at compile time (pass values through
function parameters), or its overflow lints reject them.

## Working on keel at the same time

flux depends on [keel](https://github.com/hxyulin/keel) through git,
pinned to a commit in the root `Cargo.toml`. To build against a local
checkout instead, create `.cargo/config.toml` (git-ignored):

```toml
[patch."https://github.com/hxyulin/keel"]
keel-asm = { path = "../keel/crates/keel-asm" }
keel-codegen = { path = "../keel/crates/keel-codegen" }
keel-frontend = { path = "../keel/crates/keel-frontend" }
keel-ir = { path = "../keel/crates/keel-ir" }
keel-link = { path = "../keel/crates/keel-link" }
keel-opt = { path = "../keel/crates/keel-opt" }
```

Update the pinned commit once the keel changes are on keel's `main`.

## Issues and pull requests

Use the issue forms for bugs, feature requests and documentation problems.
For parser bugs, include the smallest reproducing source, what rustc does
with it, and what flux does.

Describe the problem and the resulting behavior in your pull request, and
list the checks you ran.
