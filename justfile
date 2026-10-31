# Build, check and explore the parser. `just` with no arguments lists recipes.

default:
    @just --list

# Debug build of flux.
build:
    cargo build
    target/debug/fluxc build-builtins

# Compile selected compiler modules with rustc and flux and compare execution.
selfhost-subset:
    cargo test --release -p flux-cli --test run compiler_modules_match_rustc -- --nocapture

# Format, lint, test, then check that the workspace and examples parse without errors.
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    python3 -m unittest discover -s scripts -p 'test_*.py'
    python3 scripts/test.py all
    cargo run -q -p flux-cli --bin fluxc -- check examples/*.rs $(find crates -name '*.rs' -not -path '*/tests/data/*')

# Run a named test group; use --target, --case or --filter for a narrower selection.
test group="all" *args:
    python3 scripts/test.py {{group}} {{args}}

# Small smoke plan for all components or one named group; skips exhaustive suites.
test-fast component="all" *args:
    python3 scripts/test.py {{component}} --fast {{args}}

# All unit/integration tests belonging to one workspace package, e.g. flux-mir.
test-component package *args:
    python3 scripts/test.py --package {{package}} {{args}}

# Select tests affected by tracked and untracked changes since a Git revision.
test-changed base="HEAD" *args:
    python3 scripts/test.py --changed {{base}} {{args}}

# Rewrite parser snapshots after an intended change; review the diff afterwards.
snapshots:
    UPDATE_EXPECT=1 cargo test -p flux-syntax --test parser

# Regenerate the typed AST from crates/flux-syntax/flux.ungram.
sourcegen:
    cargo run -q -p flux-sourcegen

# Parse every .rs file under a directory and summarize errors. Usage: just corpus ~/.cargo/registry/src
corpus dir:
    FLUX_CORPUS="{{dir}}" cargo test -p flux-syntax --release --test corpus -- --ignored --nocapture
    FLUX_CORPUS="{{dir}}" cargo test -p flux-sourcegen --release --test ast -- --ignored --nocapture
    FLUX_CORPUS="{{dir}}" cargo test -p flux-hir -p flux-ty -p flux-mir --release -- --ignored corpus --nocapture

# Per-phase parser throughput over every .rs file under a directory. Usage: just bench ~/.cargo/registry/src 18
bench dir threads="1":
    cargo run -q --release -p flux-syntax --example bench -- "{{dir}}" {{threads}}

# Run the crates that contain unsafe code under Miri (needs a nightly toolchain with miri).
miri:
    cargo +nightly miri test -p flux-arena -p flux-intern
    MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly miri test -p flux-arena -p flux-intern --lib

# Print the syntax tree of a file.
parse file:
    cargo run -q -p flux-cli --bin fluxc -- parse "{{file}}"

# Print the HIR of a file: items and lowered function bodies.
hir file:
    cargo run -q -p flux-cli --bin fluxc -- hir "{{file}}"

# Type-check a file and print its MIR.
mir file:
    cargo run -q -p flux-cli --bin fluxc -- mir "{{file}}"

# Compile a program with flux and run it.
run file:
    cargo run -q -p flux-cli --bin fluxc -- run "{{file}}"

# Print the tokens of a file.
lex file:
    cargo run -q -p flux-cli --bin fluxc -- lex "{{file}}"

# Differential testing: generate N programs, compile them with rustc and flux, compare. Usage: just difftest 2000 [--reduce] [--weight name=N]
difftest n="1000" *args:
    cargo build -q --release -p flux-cli -p flux-difftest
    ./target/release/difftest run -n {{n}} {{args}}

# Minimize a failing difftest seed (or `--file f.rs`), keeping its failure. Usage: just difftest-reduce 42
difftest-reduce *args:
    cargo build -q --release -p flux-cli -p flux-difftest
    ./target/release/difftest reduce {{args}}

# Lint GitHub Actions workflows (requires actionlint).
workflows-check:
    actionlint -color
