# Bounded Rust 2021 source dependencies

The package driver accepts edition 2021 and 2024 crates in one source graph.
Standalone file commands still default to edition 2024. `SourceDb` records each
file's edition; external modules inherit it, expansion files preserve it, and
`CrateData` exposes it to semantic analysis. Embedded libraries stay edition
2024. Reusing one loaded source path under different editions is rejected.

This is a bounded subset of 2021, intended to permit unchanged libraries such
as the Cargo.lock dependency `unicode-ident 1.0.27`. It is not full 2021 support.

## Edition-sensitive behavior

- `gen` is an identifier in 2021 and a reserved keyword in 2024. Raw `r#gen`
  remains available in either edition. Lexing and expansion parsing receive
  the source crate edition; source text is never rewritten.
- Plain `extern "C"` blocks are accepted in 2021. Edition 2024 still requires
  `unsafe extern "C"`. Foreign functions still require unsafe calls unless
  explicitly declared safe.
- `if let`, `while let`, and let chains are explicitly rejected in 2021. This
  avoids applying 2024's shorter scrutinee temporary scopes to 2021 code; let
  chains are also unavailable in Rust 2021 itself.
- Any inferred expression value that requires dropping is rejected in 2021,
  including unconstrained generic values that might require dropping after
  monomorphization. Thus the current 2024 MIR temporary scopes cannot silently
  reorder observable 2021 destructors. This deliberately also rejects many
  ordinary, valid 2021 functions. Synthetic tuple constructors are exempt:
  they only move parameters into the returned aggregate and create no scoped
  temporaries. Calls constructing a drop-requiring result remain checked.
- Drop-free borrowing remains subject to the existing 2024 lifetime checks.
  These can reject valid 2021 code whose tail temporaries need a longer scope.
  General 2021 tail-expression scope extension is not implemented.
- Binding modifiers or reference patterns under inherited match ergonomics
  are rejected, with an unsupported-2021 diagnostic. Fully explicit patterns
  and unmodified implicit reference bindings retain the common behavior.
- All `impl Trait` syntax is rejected in 2021, including argument position.
  In particular, 2024 RPIT implicit lifetime capture is never silently applied
  to written 2021 return-position opaque types.
- Unresolved diverging inference variables that would need never fallback are
  rejected in 2021, rather than selecting 2024's `!` fallback where 2021 would
  select `()`. Explicitly constrained diverging code is still permitted.
- `macro_rules!` definitions and invocations are rejected in 2021. This avoids
  silently using 2024 expression-fragment matching or mixing edition-sensitive
  expanded constructs. Compiler builtins remain available and their results
  still undergo type checking. Exported macros used by a 2024 consumer are a
  separate compiler feature.
- Other existing strict 2024 restrictions can reject valid 2021 source, for
  example unsafe-attribute requirements, static-mut references, and reserved
  guarded-string syntax. Async/Future/IntoFuture are outside the current
  language subset. The currently implemented prelude has no 2024-only future
  traits; Box iteration requiring a drop value falls outside the above subset.

The official Rust Edition Guide documents the reasons for these boundaries:
[`gen`](https://doc.rust-lang.org/edition-guide/rust-2024/gen-keyword.html),
[extern blocks](https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-extern.html),
[match ergonomics](https://doc.rust-lang.org/edition-guide/rust-2024/match-ergonomics.html),
[if-let scopes](https://doc.rust-lang.org/edition-guide/rust-2024/temporary-if-let-scope.html),
[tail scopes](https://doc.rust-lang.org/edition-guide/rust-2024/temporary-tail-expr-scope.html),
[RPIT capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html),
[let chains](https://doc.rust-lang.org/edition-guide/rust-2024/let-chains.html), and
[never fallback](https://doc.rust-lang.org/edition-guide/rust-2024/never-type-fallback.html).

## Normalized dependency manifests

A dependency selects exactly one ordinary `lib` unit. Its unrelated binary,
example, test and benchmark targets are not compiled. Dev dependencies,
including target-specific dev dependencies, are not visited or resolved.
The manifest itself is preserved, and `cargo metadata --offline --no-deps`
validates target discovery without compiling anything. This follows Cargo's
[unit/target distinction](https://doc.rust-lang.org/cargo/reference/cargo-targets.html)
and [development-dependency rules](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#development-dependencies).
The executable root retains its previous binary-only target restrictions.

The driver accepts package `rust-version`, opaque `metadata`, target discovery
booleans, and `build = false`. The latter disables even an existing `build.rs`,
as specified by [Cargo's manifest format](https://doc.rust-lang.org/cargo/reference/manifest.html#the-build-field).
No build script is executed. `build = true`, script paths, build dependencies,
proc macros, nonempty features, workspace declarations/inheritance, Cargo
configuration and normal target-specific dependencies remain rejected.
Normal dependencies must still be explicit local paths (optionally renamed).
There is no registry solver, and no manifest or source normalization pass.

## Reproduce validation

The positive mixed-edition source uses a `.rs.txt` suffix because workspace
source checks otherwise parse every `.rs` file as 2024. It is
`crates/flux-cli/tests/fixtures/edition2021/lib.rs.txt`; its child source, binary
source, normalized-style manifests and rustc invocations are in
`crates/flux-cli/tests/edition2021.rs`. The test compares a Flux executable with
a rustc 2021 rlib linked into a rustc 2024 binary, and checks that the same
library is rejected under rustc 2024 (`gen` and plain extern blocks).

`crates/flux-cli/tests/fixtures/edition2021/drop_order.rs` is compiled and run
under both rustc editions. Rust 2021 prints `drop=1`, `drop=2`, `value=2`;
Rust 2024 prints `drop=2`, `drop=1`, `value=2`. Flux must explicitly reject the
2021 dependency instead of choosing the second ordering.
`if_let_drop_order.rs` similarly verifies `else` before `drop` in 2021 and
`drop` before `else` in 2024. Further negative fixtures cover let chains, match
modifiers, opaque types, macros and never fallback, along with active manifest options that must stay unsupported.

```sh
cargo test -p flux-cli --test edition2021
cargo test -p flux-cli --test package
cargo test -p flux-syntax --test editions
cargo test -p flux-cli cached_unicode_ident_normalized_manifest -- --ignored
cargo test -p flux-syntax -p flux-sourcegen -p flux-hir -p flux-ty -p flux-cli
cargo clippy -p flux-cli -p flux-hir -p flux-syntax -p flux-ty --all-targets -- -D warnings
cargo fmt --all -- --check
```

The explicitly ignored cache test requires the real
`~/.cargo/registry/src/*/unicode-ident-1.0.27` (or `$CARGO_HOME/registry/src/...`).
It loads the original normalized manifest through a temporary 2024 root,
checks the selected library's edition and path, and verifies the dependency's
manifest and both Rust source files are byte-identical afterward. It does not
require downloading or compiling the crate's dev dependencies.

At the base commit, compiling that unchanged dependency additionally needs
`no_std`, `rustfmt::skip`, `repr(align)`, and slice `get_unchecked` support.
Those changes are separate from edition and package-unit selection. The cache
planning test passes independently; it does not claim full codegen succeeds
before those compiler changes are integrated.
