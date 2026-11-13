# Crate graph surveys

These surveys compare unchanged registry library sources with Rustc on the current
host. They require Python 3.11 or newer and a populated Cargo registry source and
index cache. They run offline and record resolution failures separately. Procedural
macro roots and packages without library targets are explicit exclusions. Proc-macro
dependencies use Rustc hosts; this does not qualify Flux-built macro hosts.

Build the compiler and stage its library before starting a survey:

```sh
cargo build --profile test -p flux-cli --bins
target/debug/fluxc build-builtins
python3.11 scripts/corpus_graphs.py --output out/corpus-default
```

The default screen includes every installed package version, rather than only the
latest version of each name. Each case builds a library consumer with Rustc in release mode first,
then builds its Flux library graph in fast-build mode. Both profiles disable
debug assertions and use panic-abort; Cargo keeps its normal release optimization. Generic bodies are checked,
but the wrapper does not instantiate every API. Host cfg remains active, including
platform-specific stubs. This is build coverage, not runtime
coverage or a test of the upstream package's original workspace and lockfile.

Results, manifests, lockfiles and command logs live under the output directory.
The runner resumes completed cases only when the input inventory, source hashes,
compiler hashes, runner and configuration match. Ordinary temporary target folders
are removed after measurement. Externally managed target symlinks and their contents
are left untouched. Original registry source hashes are checked after the run.

Run the pinned feature configurations and selected native consumers separately:

```sh
python3.11 scripts/corpus_graphs.py --mode matrix \
  --matrix tests/corpus/feature-matrix.json --output out/corpus-features
python3.11 scripts/corpus_consumers.py \
  --config tests/corpus/feature-consumers.json \
  --inventory out/corpus-default/run.json --output out/corpus-native
```

The feature matrix covers no-default-features and selected combinations. It does
not cover every feature combination. Dependency features unify per graph, so an
isolated library and a larger consumer can exercise different code. The native consumers compare stdout, stderr,
and successful exit status with Rustc in debug, fast-build and release modes. The
sources avoid observing differences in debug assertions and overflow configuration.
UUID generation is checked without comparing random UUID values between executions.

Survey commands record failures and finish; they are not CI pass/fail gates.
The successful UUID v4/Serde consumer is also a maintained regression in
`flux-cli::corpus_crates`. Other feature consumers currently expose compilation
blockers and remain survey inputs.
