# Standalone package builds

On arm64 macOS, Flux can build a restricted Cargo package:

```sh
flux package build --manifest-path /path/to/Cargo.toml [--bin NAME]
flux package run --manifest-path /path/to/Cargo.toml [--bin NAME] -- PROGRAM_ARGS
```

This is a manifest driver. It runs real `cargo metadata --offline --no-deps
--format-version 1` from the package directory, selects the actual binary
name and source path, then compiles its source and file modules through
Flux and Keel. It never asks rustc to compile source, never falls back to
rustc, and does not implement `RUSTC`, Cargo's compiler protocol, rustc
library metadata, or general Cargo compatibility. Cargo must be installed.
Flux's existing language and embedded-library limitations still apply.

The supported manifest subset is deliberately narrow:

- Exactly one standalone package, with explicit `edition = "2024"`.
  Workspace manifests, workspace membership and inherited fields are
  unsupported, including a single-member workspace.
- Package keys: `name`, `version`, `edition`, `authors`, `description`,
  `license`, `license-file`, `readme`, `repository`, `homepage`,
  `documentation`, `keywords`, `categories`, `publish`. Cargo validates
  these fields; descriptive fields have no compiler effect.
- Inferred binary targets and explicit `[[bin]]` tables with only `name`
  and `path`. Select with `--bin` when there is more than one binary.
  `default-run` is unsupported. The selected name, with hyphens changed
  to underscores, is the compilation's crate name.
- All discovered targets must be edition-2024 binaries. Libraries,
  proc macros, examples, tests, benches and custom-build targets are
  rejected even when a supported binary was selected.
- No other top-level declarations or package/binary options. In
  particular, dependencies (normal, dev, build, target-specific, optional,
  even empty declaration tables), features, profiles, target tables,
  patches, replacements, lints and Cargo unstable options are rejected.
  A root `build.rs` is rejected; `package.build` is unsupported even when
  set to `false`.
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
  `-O`, output overrides and timing options) are rejected by the CLI.

Package builds always use the current Flux host configuration: edition
2024, debug assertions enabled, panic abort, no user features, no test
configuration, no optimization and no emitted debug information. Cargo's
default dev/release profiles are not applied. There is no configurable
compilation mode in this milestone; a per-compilation HIR configuration
API is deferred until a second configuration is supported.

The executable is written to `<package>/target/flux/<binary-name>`, separate
from Cargo's own artifacts. Every invocation rebuilds, including all
source modules; no freshness or incremental cache is claimed. Cargo may
create/update the package's lockfile while reading metadata. A failed
build does not run a previously built executable. A package run inherits
the invoking working directory and environment, forwards arguments after
`--`, and returns the program's exit status (128 plus signal on Unix).
Existing `flux build FILE` and `flux run FILE` retain their behavior.

Validate with `cargo test --offline -p flux-cli --test package`. The tests
use real Cargo and rejecting rustc/wrapper scripts to detect compiler
invocations; arm64 macOS tests also compile and execute through Flux/Keel.
