# Package builds with local source dependencies

On arm64 macOS and x86-64 Linux, Flux can build a restricted Cargo package:
Linux requires an installed glibc C toolchain for runtime-file discovery.

```sh
flux package build --manifest-path /path/to/Cargo.toml [--bin NAME] [-O] [--timings]
flux package run --manifest-path /path/to/Cargo.toml [--bin NAME] [-O] [--timings] -- PROGRAM_ARGS
```

This is a manifest driver. Flux reads `Cargo.toml` directly, selects the binary
name and source path, and compiles its source, file modules and supported local
path dependencies through Flux and Keel. It does not invoke Cargo or resolve
registry/git packages. Ordinary target source always compiles with Flux. Local
macro hosts use the explicit host compiler described below. Use
`--macro-host-compiler flux` for supported dependency-free macro hosts when
rustc is also unavailable. This driver does not implement `RUSTC`, Cargo's
compiler protocol, rustc library metadata, or general Cargo compatibility.
Flux's existing language and embedded-library limitations still apply.

`--timings` prints compilation scopes to stderr, including package planning,
manifest loading and validation, source loading and backend phases. The package elapsed scope stops before program execution.
Nested scopes overlap and must not be added together; see the
[timing boundaries](ARCHITECTURE.md#compilation-timings).

The supported manifest subset is deliberately narrow:

- One standalone binary package and zero or more standalone local library
  dependencies, each with explicit edition 2018, 2021 or 2024. Edition 2021
  uses the deliberately restricted [documented subset](EDITION_2021.md).
  Workspace manifests, workspace membership and inherited fields are
  unsupported, including a single-member workspace. Enclosing workspaces are
  rejected even when their exclusion lists would make a package standalone.
- Package keys: `name`, `version`, `edition`, `authors`, `description`,
  `license`, `license-file`, `readme`, `repository`, `homepage`,
  `documentation`, `keywords`, `categories`, `publish`, `rust-version`,
  opaque `metadata`, target discovery booleans and `build = false`. Flux
  validates the supported field types; descriptive fields have no compiler effect.
- Inferred binary targets and explicit `[[bin]]` tables with only `name`
  and `path`. Select with `--bin` when there is more than one binary.
  `default-run` is unsupported. The selected name, with hyphens changed
  to underscores, is the compilation's crate name. `autobins = false` disables
  inferred binaries while keeping explicit targets. Files named `src/bin/NAME.rs`
  and directories named `src/bin/NAME/main.rs` supply inferred binaries.
- All discovered root targets must be supported binaries. Each dependency
  must expose exactly one ordinary library target, inferred from `src/lib.rs`
  or configured with `[lib]` containing only `name` and `path`. Unselected
  dependency binary/example/test/benchmark targets and their dev dependencies
  are not compiled or resolved. Local procedural macro dependencies may use
  `[lib] proc-macro = true`. Root libraries and custom-build targets remain
  unsupported.
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
  files are allowed. Configuration checks start at the package directory;
  the caller's separate configuration ancestry does not participate.
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

## Local procedural macro hosts

The standalone package driver builds local macro hosts without asking Cargo
to compile them. These hosts use the same HIR `ProcMacroExpander` interface,
macro namespace, helper handling and expansion paths as Cargo-built macros.
The Cargo-backed `flux build --cargo` driver retains its toolchain-server behavior.

The default local host compiler is rustc, invoked directly. To compile a
dependency-free macro host and its token runtime with Flux and Keel, use:

```sh
flux package run --manifest-path /path/to/Cargo.toml --macro-host-compiler flux
```

Flux mode does not invoke rustc and has no fallback. Its hosts support the
language and library subset Flux can compile. Package dependencies, qualified
or generic entry functions, and an enabled private `main` remain unsupported.
The default mode can compile transitive local host libraries and native
proc-macro dependencies used by those libraries or the macro root.

The host-side `flux-proc-macro` crate supplies the replacement `proc_macro`
token API. It is not a HIR dependency. Tokens preserve punctuation spacing,
source spans, lifetimes and transparent groups across a bounded binary file
protocol. Identifier constructors use Unicode XID classes and NFC.
`proc_macro::is_available()` reports the invocation context on the current
thread; it is false on threads spawned by the macro. `Span::source_text()`
returns `None` because the protocol carries ranges without source text.
`Literal::byte_character`, `byte_string` and `c_string` preserve literal bytes;
C strings support Unicode, escaped controls and non-UTF-8 bytes. Advanced
span hygiene operations remain unsupported. CLI semantic commands reject
non-NFC physical source identifiers until semantic name interning normalizes
them consistently; string and documentation contents remain unchanged.

Nested modules, `#[path]`, helper source/data includes and conditional exports
are supported. Export discovery reads explicit root functions. Entries
generated by source inclusion or declarative macros remain unsupported in
local mode. Helpers exposing sysroot `proc_macro` types cannot yet be shared
with the replacement token API. `syn`, `quote`, Serde and Clap derive support
is not implied. The unchanged published `default_new_macros 0.1.0` derive is
compared with rustc through both local host compilers.

Local hosts run in a new process for each invocation, with the invocation
environment overrides and current directory supplied by main's HIR interface.
Protocol files are separate from stdout and stderr. Expansions have a
60-second deadline, a 64 MiB response limit and token/depth limits. Hosts are
not sandboxed. Host compilation in the default mode supplies the macro
package's `CARGO_MANIFEST_DIR`; Flux-built hosts use the compiler invocation's
environment for compile-time environment macros.

Artifacts live under `<package>/target/flux-host` and rebuild on every package
invocation. Target executables remain under `target/flux`. The local host
adapter does not change Cargo's persistent host state or its toolchain ABI.

## Package execution

`#![no_std]` library roots use core's type and macro preludes, without implicit
std/alloc types or printing macros. Explicit `extern crate alloc` or
`extern crate std` can opt into their APIs. Freestanding executable support
is not added: a no_std executable currently requires `extern crate std`.

Package builds retain each crate's selected edition and use Flux's current
host configuration: panic abort, no user features, no test configuration and
no emitted debug information. Backend optimization is off by default; `-O`
enables Keel optimization. `cfg(debug_assertions)` follows the manifest's
Cargo profile: `[profile.release] debug-assertions` with `-O` and
`[profile.dev] debug-assertions` without, defaulting to off and on as Cargo
does. `flux build` reads the same setting from the workspace manifest and also
applies it to build scripts and their `CARGO_CFG_DEBUG_ASSERTIONS`. Other
profile settings are not applied.

The executable is written to `<package>/target/flux/<binary-name>`, separate
from Cargo's own artifacts. Every invocation rebuilds, including all
source modules and dependencies; no freshness or incremental cache is
claimed. Flux does not read or change `Cargo.lock`; local path manifests define
the graph. A failed build does not run a previously built executable. A package
run inherits the invoking working directory and environment, forwards arguments after
`--`, and returns the program's exit status (128 plus signal on Unix).
Existing `fluxc build FILE` and `fluxc run FILE` retain their behavior.

Validate with `cargo test --offline -p flux-cli --test package`. The tests
block Cargo and rustc with rejecting executables on `PATH`; arm64 macOS tests
also compile and execute a path-library graph with a Flux-built proc macro.
Separate reference checks may use Cargo to locate cached upstream sources.

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

Unchanged `rustc-hash 2.1.3` sources compile through an explicit feature-free
edition-2024 wrapper. Default and seeded hashing agree with rustc for 129
lengths at four offsets, plus tuple/string hashing, in both optimization modes.
The package test rejects any rustc invocation during Flux builds; rlibs are
created only by the separate reference compiler. Its original manifest's
`default-features` option and edition-2021 while-let still remain unsupported.
