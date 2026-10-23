# Package builds with local source dependencies

On arm64 macOS, Flux can build a restricted Cargo package:

```sh
flux package build --manifest-path /path/to/Cargo.toml [--bin NAME] [-O] [--timings]
flux package run --manifest-path /path/to/Cargo.toml [--bin NAME] [-O] [--timings] -- PROGRAM_ARGS
```

This is a manifest driver. It runs real `cargo metadata --offline --no-deps
--format-version 1` from the package directory, selects the actual binary
name and source path, then compiles its source, file modules and supported
local path dependencies through Flux and Keel. Metadata is read separately for each package; this
bounded path-only graph does not use Cargo's general resolved feature graph.
It never asks rustc to compile source, never falls back to rustc, and does not implement `RUSTC`, Cargo's compiler protocol, rustc
library metadata, or general Cargo compatibility. Cargo must be installed.
Flux's existing language and embedded-library limitations still apply.

`--timings` prints compilation scopes to stderr, including package planning,
each offline metadata invocation with decoding/validation, source loading and
backend phases. The package elapsed scope stops before program execution.
Nested scopes overlap and must not be added together; see the
[timing boundaries](ARCHITECTURE.md#compilation-timings).

The supported manifest subset is deliberately narrow:

- One standalone binary package and zero or more standalone local library
  dependencies, each with explicit edition 2021 or 2024. Edition 2021
  uses the deliberately restricted [documented subset](EDITION_2021.md).
  Workspace manifests, workspace membership and inherited fields are
  unsupported, including a single-member workspace.
- Package keys: `name`, `version`, `edition`, `authors`, `description`,
  `license`, `license-file`, `readme`, `repository`, `homepage`,
  `documentation`, `keywords`, `categories`, `publish`, `rust-version`,
  opaque `metadata`, target discovery booleans and `build = false`. Cargo validates
  these fields; descriptive fields have no compiler effect.
- Inferred binary targets and explicit `[[bin]]` tables with only `name`
  and `path`. Select with `--bin` when there is more than one binary.
  `default-run` is unsupported. The selected name, with hyphens changed
  to underscores, is the compilation's crate name.
- All discovered root targets must be supported binaries. Each dependency
  must expose exactly one ordinary library target, inferred from `src/lib.rs`
  or configured with `[lib]` containing only `name` and `path`. Unselected
  dependency binary/example/test/benchmark targets and their dev dependencies
  are not compiled or resolved. Root libraries, proc macros and custom-build
  targets remain unsupported.
- Normal `[dependencies]` entries support only `{ path = "..." }` and
  `{ path = "...", package = "..." }`. Transitive dependencies, renamed
  imports and shared diamond dependencies are supported. Canonical manifest
  paths identify crates; cycles and colliding import names are rejected.
  An unrenamed dependency uses its library target name; `package` selects
  a renamed dependency whose table key becomes the import name. Hyphens
  become underscores in Rust import names. The names `core`, `alloc` and
  `std` are reserved for the embedded libraries.
- No other top-level declarations or package/binary options. In
  particular, registry/git/version dependencies, build dependencies, normal
  target-specific dependencies, optional dependencies, feature/default-feature
  settings, nonempty declared features, profiles, patches, replacements, lints
  and Cargo unstable options are rejected. Root dev dependencies are also
  rejected. A `build.rs` is rejected unless `package.build = false` disables it.
- No nonempty Cargo configuration files: both `.cargo/config` and
  `.cargo/config.toml` in the package directory or any ancestor, plus
  `config` and `config.toml` in `CARGO_HOME` (default `$HOME/.cargo`), are
  parsed as TOML and rejected if they contain entries. An explicit
  `CARGO_HOME` must be absolute. Empty/comment-only
  files are allowed. Cargo is invoked from the package directory, so the
  caller's separate configuration ancestry does not participate.
- Environment options `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`,
  `RUSTDOCFLAGS`, `CARGO_ENCODED_RUSTDOCFLAGS`, `CARGO_INCREMENTAL`,
  and names beginning `CARGO_BUILD_`, `CARGO_TARGET_`, `CARGO_PROFILE_`
  or `CARGO_UNSTABLE_` are rejected, including empty values.
  Unknown command options (including `--target`, `--release`, `--features`,
  output overrides) are rejected by the CLI.

Exported `macro_rules!` macros support dependency aliases and transitive
reexports. `$crate` retains the defining crate's identity through nested,
forwarded and generated macros, including item and type expansion. It does
not bypass privacy. Item expansion is still sequential: lookup that requires
later imports or a general expansion/resolution fixed point may be rejected.
Predecessor lookup is checked against the completed module scopes so later
shadowing cannot silently select the wrong macro.

`#![no_std]` library roots use core's type and macro preludes, without implicit
std/alloc types or printing macros. Explicit `extern crate alloc` or
`extern crate std` can opt into their APIs. Freestanding executable support
is not added: a no_std executable currently requires `extern crate std`.

Package builds retain each crate's selected edition and use Flux's current
host configuration: debug assertions enabled, panic abort, no user features, no test
configuration and no emitted debug information. Backend optimization is off
by default; `-O` enables Keel optimization without selecting a Cargo release
profile or changing `cfg(debug_assertions)`. Cargo's
default dev/release profiles are not applied. There is no configurable
compilation mode in this milestone; a per-compilation HIR configuration
API is deferred until a second configuration is supported.

The executable is written to `<package>/target/flux/<binary-name>`, separate
from Cargo's own artifacts. Every invocation rebuilds, including all
source modules and dependencies; no freshness or incremental cache is
claimed. Cargo may create/update the package's lockfile while reading metadata. A failed
build does not run a previously built executable. A package run inherits
the invoking working directory and environment, forwards arguments after
`--`, and returns the program's exit status (128 plus signal on Unix).
Existing `flux build FILE` and `flux run FILE` retain their behavior.

Validate with `cargo test --offline -p flux-cli --test package`. The tests
use real Cargo and rejecting rustc/wrapper scripts to detect compiler
invocations; arm64 macOS tests also compile and execute through Flux/Keel.

For example, a binary can use a local library:

```toml
[dependencies]
helpers = { path = "../helpers" }
```

```rust
fn main() {
    println!("{}", helpers::answer());
}
```

Flux loads dependency sources as separate crates with their own roots and
direct dependency scopes. Privacy, inherent-impl ownership, trait orphan
rules and crate/type identity are checked; a transitive dependency is not implicitly importable. All source bodies are
checked, then the reachable program is monomorphized and emitted through Keel
as one executable. Internal function/static symbols distinguish crates even
when their library target names match. No dependency `.rlib` or object
artifact is produced.
Separate compilation would additionally need Flux metadata for types, generics
and symbol identities; rustc `.rlib` metadata is not consumed.

The package tests also compile the unchanged compiler `rng.rs` as an ordinary
path library, exercise a generic call through a renamed diamond graph, and
verify dependency edits and failures cannot silently reuse an old executable.

The original cached `unicode-ident 1.0.27` package is compiled as a local
path dependency using its unchanged edition-2021 manifest and sources.
Classification counts and a digest across every Unicode scalar match rustc
with and without Flux optimization. The edition-2024 source-wrapper test
also remains. This does not add registry resolution or compilation of its
dev dependencies.

Unchanged `memchr 2.8.3` sources also compile and run through an explicit
edition-2024, feature-free path-library wrapper. A 32,640-case comparison covers
offsets, lengths, byte-search variants, counts, substring searches, cloned
finders and forward/reverse iterators; rustc and Flux agree with and without
optimization. Flux loads the dependency sources directly; it does not produce
or consume Rust rlibs. The test's rlibs belong only to the rustc oracle.
The original memchr manifest is still unsupported: feature selection, optional
registry dependencies and broader edition-2021 semantics remain separate work.
