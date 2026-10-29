<h1 align="center">flux</h1>
<p align="center">An experimental compiler for a subset of Rust, built on the Keel backend.</p>
<p align="center">
  <a href="rust-toolchain.toml"><img alt="Rust stable" src="https://img.shields.io/badge/Rust-stable-dea584?logo=rust"></a>
  <a href="Cargo.toml"><img alt="Rust edition 2024" src="https://img.shields.io/badge/edition-2024-dea584"></a>
  <a href="docs/SELF_HOSTING.md"><img alt="Target arm64 macOS" src="https://img.shields.io/badge/target-arm64%20macOS-lightgrey?logo=apple"></a>
  <a href="https://github.com/hxyulin/keel/commit/103c23de480d76e02267c5a8d5a9d3505bc73231"><img alt="Keel 103c23de" src="https://img.shields.io/badge/Keel-103c23de-6e56cf"></a>
  <a href="#size"><img alt="About 520k lines of compiler source" src="https://img.shields.io/badge/source-~520k%20lines-informational"></a>
  <a href="https://github.com/hxyulin/flux-lang/actions/workflows/ci.yml"><img alt="CI status" src="https://github.com/hxyulin/flux-lang/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="#license"><img alt="MIT or Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue"></a>
</p>
<p align="center">
  <a href="docs/ARCHITECTURE.md">Architecture</a> ·
  <a href="docs/LANGUAGE.md">Supported language</a> ·
  <a href="docs/SELF_HOSTING.md">Self-hosting</a> ·
  <a href="docs/ROADMAP.md">Roadmap</a> ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

flux compiles Rust source to native executables through
[Keel](https://github.com/hxyulin/keel), its own compiler backend, with no
LLVM and no rustc in the build. It accepts only programs that are also valid
Rust, so `rustc` serves as the test oracle: every program flux compiles must
produce the same output and exit status under both. flux exists to learn how
Rust's toolchain works, to measure where its costs come from, and to
stress-test Keel with Rust-shaped code.

| Area | flux supports |
| --- | --- |
| Language | Edition 2024, with bounded 2015, 2018 and 2021 modes; generics, traits, trait objects, closures, `impl Trait`, unions and `macro_rules!` |
| Checking | Type inference, trait solving, a borrow checker, constant evaluation and rustc's overflow lints |
| Libraries | Its own `core`, `alloc` and a bounded `std`, written in flux, including threads, atomics, `OnceLock` and collections |
| Real crates | Unchanged sources of hashbrown, smallvec, typenum, bumpalo, memchr, crc32fast and more, matched against rustc |
| Builds | One `.flib` artifact per crate, a prebuilt built-in library, local Cargo packages and procedural macros |
| Backend | Keel objects for arm64 macOS and x86-64 Linux, linked into executables on arm64 macOS |

Not supported yet: `async`, unwinding (panics abort), and most of the
registry and feature resolution that Cargo does. The
[language reference](docs/LANGUAGE.md) lists every restriction.

## Self-hosting

flux has compiled itself. A stage-1 build loaded the complete compiler: the
CLI library and binary across 70 package identities, Keel included. The
resulting compiler matched rustc's build on its help and version output,
then compiled and ran a smoke program at both optimization levels. That run
used Keel `f37f0bd` with two vector-assembly fixes applied locally. Both
fixes are in the pinned Keel `103c23de`.

The pinned Keel has since grown, adding the bbtree and Verus dependencies.
That larger backend graph does not yet pass flux's front end, so stage 1 has
not been repeated on the unmodified pin. A stage-2 rebuild, flux compiling
itself with a flux-built compiler, is untested. Cargo and rustc still build
host build scripts and procedural macros. Details and every audited source
graph are in [docs/SELF_HOSTING.md](docs/SELF_HOSTING.md).

## Size

Counted with `tokei` on 2026-10-30, Rust only, vendored code excluded.

| Part | Lines |
| --- | ---: |
| flux compiler (`crates/*/src`) | 141k |
| flux libraries (`library/`) | 42k |
| Keel backend (`crates/*/src`) | 338k |
| **Compiler source, flux and Keel** | **521k** |
| flux tests | 134k |
| Keel tests, suite and tools | 172k |

`tests/run` holds 475 programs that both rustc and flux compile and run.

## Compile time

<details>
<summary><strong>Measured against rustc on one source graph</strong></summary>

The tree fixture is flux's lexer, parser and tree modules plus the real
memchr and unicode-ident crates: four crates, about 8,700 lines. Measured on
arm64 macOS on 2026-10-30, medians of three runs, against rustc 1.99.1.
flux runs as one process on one thread.

| Clean build | flux | rustc, one job | Cargo, default jobs |
| --- | ---: | ---: | ---: |
| Unoptimized | 1.18 s | 1.01 s | 0.66 s |
| Optimized | 1.64 s | 2.02 s | 0.91 s |

A hello-world build takes 0.2 s, most of it loading the prebuilt built-in
library. flux has no incremental cache inside a crate yet, so single-file
edits are several times slower than Cargo's.

</details>

## Quickstart

flux builds with stable Rust. Linking executables needs arm64 macOS.

<details>
<summary><strong>Build flux and compile a program</strong></summary>

```sh
git clone https://github.com/hxyulin/flux-lang.git
cd flux-lang
cargo build
target/debug/flux build-builtins   # prebuild the built-in library (optional)
```

```sh
target/debug/flux run tests/run/fib.rs        # compile and run a program
target/debug/flux build tests/run/fib.rs -O   # write an optimized executable
target/debug/flux mir tests/run/fib.rs        # print its MIR
target/debug/flux check examples/*.rs         # report syntax errors
target/debug/flux package build --manifest-path path/to/Cargo.toml
```

Without `build-builtins`, each run compiles the built-in library from source
first, which adds about half a second.

</details>

<details>
<summary><strong>Run the tests</strong></summary>

```sh
just test-fast              # quick smoke plan
just test-fast borrowing    # one component
just check                  # format, clippy and the complete suite
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the test runner and its options.

</details>

## Documentation

- [Architecture](docs/ARCHITECTURE.md): the pipeline and its data structures.
- [Supported language](docs/LANGUAGE.md): what flux accepts and what it rejects.
- [Self-hosting](docs/SELF_HOSTING.md): the audited source graphs and the bootstrap.
- [Library artifacts](docs/FLIB.md): `.flib` files and the built-in library.
- [Package driver](docs/PACKAGE_DRIVER.md): building local Cargo packages.
- [Roadmap](docs/ROADMAP.md): what comes next.

## Contributing

- [CONTRIBUTING.md](CONTRIBUTING.md) covers setup, the test runner and conventions.
- Report a miscompile with the smallest program where flux and rustc disagree.
- [AGENTS.md](AGENTS.md) records repository conventions for coding assistants.

## License

flux is available under either the [MIT license](LICENSE-MIT) or the
[Apache License 2.0](LICENSE-APACHE), at your option.
