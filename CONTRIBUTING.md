# Contributing to flux

flux is an experimental compiler for a subset of Rust. Start with the
[architecture guide](docs/ARCHITECTURE.md) and the
[supported language](docs/LANGUAGE.md). Small fixes, fixtures and clear bug
reports are useful contributions.

## Set up

- Rust stable, with `rustfmt` and `clippy` (selected by `rust-toolchain.toml`).
- [just](https://github.com/casey/just) for the repository recipes.
- Python 3.9 or newer for test selection and scheduling (standard library only).
- [actionlint](https://github.com/rhysd/actionlint) for workflow edits; CI pins 1.7.12.

```sh
git clone https://github.com/hxyulin/flux-lang.git
cd flux-lang
cargo build
target/debug/flux build-builtins
just check
```

`just` lists the available recipes.

The staging command writes checked built-in types and MIR beside the compiler.
It returns without rebuilding when the suite already matches. `just build`
and the test runner run it automatically; plain Cargo commands also work without
it and use the source library path. A rebuilt compiler rejects a stale suite.
Use `FLUX_BUILTINS_SOURCE=1` to force source mode, or
`FLUX_BUILTINS_TRACE=1` to report which mode a CLI compilation selects.

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
| `crates/flux-ty-ir/` | Type representation shared by the solver and the type checker: `Ty`, predicates, `Signatures` |
| `crates/flux-solve/` | Trait solving: whether predicates hold, normalization, `impl` selection |
| `crates/flux-ty/` | Type inference and checking, one body at a time |
| `crates/flux-mir/` | MIR: control-flow graphs with explicit runtime checks; initialization check |
| `crates/flux-codegen/` | MIR to keel IR, native objects and executables |
| `crates/flux-cli/tests/run.rs`, `tests/run/` | Programs built with rustc and flux; output and exit status must match |
| `crates/flux-difftest/`, `tests/difftest/bugs/` | Random programs compared against rustc; minimized reproducers of what it found |
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

### Fast edit/test loop

```sh
just test-fast                    # smoke checks across all components
just test-fast borrowing          # MIR units plus selected valid/error snapshots
just test-fast syntax             # syntax/sourcegen units, snapshots and AST freshness
just test-fast codegen            # object generation and two native programs
just test-changed HEAD --fast     # affected smoke targets
just test-component flux-mir      # all unit/integration tests in this crate
just test-component flux-ty --fast # only this crate's smoke targets
just check                        # complete validation
```

The fast plan runs unit tests and explicit samples for syntax, name resolution,
inference, borrowing, object emission and library APIs. On arm64 macOS it also
runs two end-to-end programs and the Cow/C-string native checks. It excludes
source-graph audits, scaling tests, large oracle matrices and the exhaustive
runtime sweep. `just test fast` is an alias for `just test-fast`. `just check`
runs the complete plan instead, which includes every smoke target.

The plan prints its exact function and fixture filters. Before running a smoke
target, the runner lists its tests with libtest `--list` and fails if the
function filter selects none; it also fails if a case pattern matches no case.
A filter may select any number of tests, so adding or renaming a test does not
break the plan. Each test binary has a 30-second deadline, adjustable with `--timeout`.
Compiling changed crates happens before that deadline; a fresh build can still
take longer than the tests themselves. Use `--list` to inspect a plan without
building. Filter overrides require `--target`.

`test-component` selects a Cargo package and does not add workspace dependents.
Its scope is unit/integration tests; `just check` includes the full downstream
suite and doctests. A fast changed-file plan intersects smoke and affected-target
selection. If changes select only an integration target outside the smoke plan,
it fails with instructions to omit `--fast` and run that target directly.

### Targeted tests

The test runner builds selected targets once, then runs their binaries with a shared
worker budget. Start with a group or a specific binary:

```sh
just test syntax
just test borrowing --target projected_pointer_regions
just test codegen --target native_fixtures --filter cow_as_ref
just test inference --target ty --case c_str
just test codegen --target run --case c_strings
just test codegen --target proof_compatibility --case signatures/ --filter signature_tautology
just test-changed HEAD --list
just test-changed main
just test all                 # all default tests, graph audits and doctests
```

Groups are `syntax` (syntax and source generation), `names` (HIR), `inference`
(types), `borrowing` (MIR), `codegen` (codegen and ordinary CLI tests), `library`
(ordinary CLI tests), `graphs` (large source-graph audits), and `units` (library
and binary unit tests). `all` includes every group. Named groups are focused
checks, not a claim that downstream behavior is unaffected.

`--changed BASE` includes tracked edits, deletions and untracked files. Production
edits select the owning crate and all workspace dependents, including graph tests.
An independent integration-test edit selects that binary. Shared test fixtures
select the crate and its dependents. A `tests/run` program selects the runtime
sweep and every test file that names it, by path or by quoted file stem; files
under `tests/run/support` select all CLI tests. `scripts/` edits select the
runner's unit tests (`scripts::runner`). Markdown files, `docs/`, and untracked
files outside `crates/`, `library/`, `tests/`, `scripts/` and `examples/` select
nothing. Embedded-library, configuration and other unknown paths select the
entire suite. Explicit group, `--package` and `--target` restrictions intersect
this selection. Review `--list` before narrowing it further. No changed paths
means no tests run.

`--filter` selects Rust test functions. `--case` selects comma-separated case-name
substrings inside snapshot fixtures, ordinary native fixtures and named oracle
loops. It requires `--target`; unrelated standalone test functions in that binary
still run unless excluded with `--filter`. The runner fails if no case matches.
Direct Cargo commands can use `FLUX_TEST_CASE=c_str cargo test -p flux-ty --test ty
-- --nocapture`; the runner additionally checks for unmatched filters. Corpus and
source-graph inventories deliberately remain complete.

The runner builds all selected targets with one Cargo command. `--jobs N`, or
`FLUX_TEST_JOBS=N` in the environment, bounds the concurrently running test
processes; the default is three quarters of the available cores, which leaves
room for builds and other work on a shared machine. Ordinary test binaries run
their test functions serially and hold one job each. The runtime sweep holds
half the jobs, or all of them when it runs alone, and compares that many
programs at once; proof compatibility holds up to four and runs that many test
functions at once. Binaries start longest first, using the durations in recent
`results.json` files; binaries without a recorded duration start first, in
name order. Scaling tests and `flux-ty::trait_solver` assert elapsed time, so
they run one at a time after the others.

Outside fast mode, each binary's outer deadline is five times its last passing
duration, at least 300 seconds, or 900 seconds without one. `--timeout SECONDS`
sets one deadline for every binary. These add to the tests' own command
deadlines. Interrupting the runner stops active test process groups and
prevents queued tests from starting. Logs, individual timings and
`results.json` live under `out/test-runs/`; the runner keeps the 10 most recent
runs, adjustable with `--keep-runs N`. Local builds may pass `--cargo mbx`.

A complete plan that includes the unfiltered runtime sweep assigns ordinary
`support::oracle` native comparisons to that sweep. Focused suites still execute
their additional IR and rejection checks. Direct Cargo runs and filtered runner
plans perform their own native comparisons. Environment-dependent fixtures,
foreign-library tests and graph consumers keep their separate comparisons.

Thirteen ordinary native wrappers now share `native_fixtures` instead of separate
binaries. For example, use `cargo test -p flux-cli --test native_fixtures
projected_pointer_regions`. This keeps fixture names selectable while reducing
repeated linking. Proof compatibility runs semantic checking and MIR building
through the APIs once per case, with a separate CLI error-reporting smoke test.

### Test compilation profile

`cargo test` uses optimization level 1 for the compiler and test harnesses.
Debug information, debug assertions and overflow checks remain enabled. The
ordinary `cargo build` development profile is unchanged. Flux-generated programs
still run through both unoptimized and optimized codegen modes.

Changing optimization levels rebuilds artifacts that are not already cached.
Later builds reuse them. To compare or debug without optimization, override only
the optimization level:

```sh
CARGO_PROFILE_TEST_OPT_LEVEL=0 cargo test -p flux-mir --test mir
CARGO_PROFILE_TEST_OPT_LEVEL=0 python3 scripts/test.py borrowing --target mir
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
Shared CLI test mechanics live in `crates/flux-cli/tests/support`: `process`
runs bounded commands with file-backed output, `oracle` compares ordinary native
fixtures, and `inventory` records original bytes and physical diagnostic paths.
`tests/support/scratch.rs` gives each test a fresh directory that is removed
when the test passes and kept when it fails; `tests/run` and the self-host
tests use it.
Graph tests keep their dependencies, cfg and assertions explicit. Audit parents
check original source bytes even when their worker fails or times out.

The complete runtime sweep currently needs arm64 macOS. Native x86-64 Linux
has focused executable and package coverage:

```sh
cargo test -p flux-cli --test linux_native --test package --test flib_package
```

Linux links through Keel using installed glibc startup files discovered by `cc`.
The bounded runtime still lacks Linux argument capture, process spawning,
channels and recursive directory removal. x86-64 macOS and Rosetta execution
are not supported targets. Write
programs so rustc cannot fold them at compile time (pass values through
function parameters), or its overflow lints reject them.

The runtime harness also bundles the actual differential generator's
choice-tape module, MIR's dataflow bitset and interval sets, and syntax-kind
classification, parser events, generator type helpers and syntax-tree
navigation with the drivers in `tests/run/support/`. Both compilers compile
those sources and compare their behavior. Run `just selfhost-subset` for
this test alone. This covers compiler implementations as they change; it
does not yet build the entire compiler with flux. The modules are bundled
using the original bundled test arrangement. Literal `#[path]` on supported
file modules is also covered by the separate SlicePool source test. See the
[self-compilation milestones and gaps](docs/SELF_HOSTING.md).

## Differential testing

`crates/flux-difftest` generates random programs inside flux's subset,
compiles each with rustc and flux (with and without `-O`), runs them and
compares their output and exit status, as `tests/run` does for written
programs. The generator is type-directed, so its programs are valid
Rust (no undefined behavior, deterministic, terminating); each prints a
tag and a hash for every value it observes, so the first differing line
locates a miscompilation. It is too slow for `just check`; `cargo test
-p flux-difftest` only checks that rustc accepts a few seeds' programs.

```sh
just difftest 2000                 # a campaign over seeds 0..2000, all cores
just difftest 2000 --start 50000   # other seeds
just difftest 500 --reduce         # also minimize the first case of each failure
just difftest-reduce 1234          # minimize one seed's failure
just difftest-reduce --file bug.rs # minimize a program by deleting lines
```

A campaign writes `out/difftest/summary.txt` (outcomes by class, and
failures grouped by signature) and `out/difftest/cases/<seed>/`
(`program.rs`, `info.txt` with what happened and how to reproduce it,
and `reduced.rs` once reduced). The classes are `miscompile` (output or
exit status differs, with the modes it happens in), `rejected` (flux
reports an error in a program rustc accepts), `crash` (flux panics),
`backend` (keel or the linker fails), `run-timeout` and
`compile-timeout`; `invalid-program` is a generator bug.
`target/release/difftest` also has `gen SEED` (print a program), `test
SEED` (run one), `validate -n N` (rustc only, for generator work), `ir
FILE` (flux's keel IR) and `bisect-opt FILE`, which, for a failure seen
only with `-O`, finds the first pass of keel's pipeline after which the
program differs from rustc's, or reports that it differs with no passes
at all (a flux bug).

The reducer first shrinks the choices the generator made (so the
program stays valid by construction), then deletes lines while rustc
still accepts the program and it still fails with the same signature,
and formats the result. Put a minimized reproducer of each distinct bug
in `tests/difftest/bugs/NNN_short_name.rs`, with a doc comment saying
what rustc and flux do, in which modes, the seed, and a diagnosis.
`difftest weights` lists the generator's feature weights, set with
`--weight name=N` (0 turns a feature off); the last group of them keeps
constructs flux rejects for now out of programs, so that campaigns find
other bugs. Raise them once those bugs are fixed.

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
