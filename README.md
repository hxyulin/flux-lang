<h1 align="center">flux</h1>
<p align="center">An experimental compiler for a subset of Rust, built on the Keel backend.</p>
<p align="center">
  <a href="rust-toolchain.toml"><img alt="Rust stable" src="https://img.shields.io/badge/Rust-stable-dea584?logo=rust"></a>
  <a href="Cargo.toml"><img alt="Rust edition 2024" src="https://img.shields.io/badge/edition-2024-dea584"></a>
  <a href="docs/SELF_HOSTING.md"><img alt="Target arm64 macOS" src="https://img.shields.io/badge/target-arm64%20macOS-lightgrey?logo=apple"></a>
  <a href="docs/SELF_HOSTING.md"><img alt="Target x86-64 Linux" src="https://img.shields.io/badge/target-x86--64%20Linux-lightgrey?logo=linux"></a>
  <a href="https://github.com/hxyulin/keel/commit/a078986ccddb1394db73b42843b282f271807ae4"><img alt="Keel a078986c" src="https://img.shields.io/badge/Keel-a078986c-6e56cf"></a>
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
[Keel](https://github.com/hxyulin/keel), its own compiler backend, without
LLVM in its code-generation path. It accepts only programs that are also valid
Rust, so `rustc` serves as the test oracle: every program flux compiles must
produce the same output and exit status under both. flux exists to learn how
Rust's toolchain works, to measure where its costs come from, and to
stress-test Keel with Rust-shaped code.

| Area | flux supports |
| --- | --- |
| Language | Edition 2024, with bounded 2015, 2018 and 2021 modes; generics, traits, trait objects, closures, `impl Trait`, unions and `macro_rules!` |
| Checking | Type inference, trait solving, a borrow checker, constant evaluation and rustc's overflow lints |
| Libraries | Its own `core`, `alloc` and a bounded `std`, written in flux, including threads, atomics, `OnceLock` and collections |
| Real crates | Unchanged sources of hashbrown, smallvec, typenum, bumpalo, memchr, crc32fast and more, matched against rustc on arm64 macOS |
| Builds | One `.flib` artifact per crate, a prebuilt built-in library, local Cargo packages, locked dependency fetching and procedural macros |
| Backend | Native executables on arm64 macOS and x86-64 Linux, linked in-process with Keel |

Not supported yet: unwinding (panics abort), dependency version resolution,
and lockfile updates. The
[language reference](docs/LANGUAGE.md) lists every restriction.

## Self-hosting

On arm64 macOS and x86-64 Linux, flux has built optimized stage 1 and stage 2
from the complete compiler source graph with pinned Keel `a078986c`, bbtree and
Verus. Stage 1 is built by a rustc-built Flux; stage 2 is built by stage 1.
The proofs use commit `1aacb10` plus the recorded library and frontend support
patches described in [docs/SELF_HOSTING.md](docs/SELF_HOSTING.md).

On each host, stage 2 is byte-for-byte identical to stage 1 and passed
help/version checks. Linux stage 2 passed ten compile/run comparisons in both
output optimization modes against rustc; the macOS proof covers seven programs
in both modes. The inventoried inputs stayed unchanged. Earlier macOS proofs
also cover building the compiler without `-O`.

x86-64 Linux supports native executable and local package builds, including
library artifact reuse. Process spawning, owned pipes and channels have native
Linux comparisons. Integer SSE/AVX2, the AVX-512 CRC folding subset, SHA-256 instructions and runtime
CPU detection have native Linux comparison coverage. Argument capture, recursive removal and
bounded integer inline assembly also run on Linux. x86-64 macOS and
Rosetta are not supported targets. The audited stage 2 proofs used Cargo and
rustc for host build scripts and procedural macros. Native offline builds now
prepare those host tools directly with rustc through `flux cargo build --native`;
see [native source builds](crates/flux-cli/README.md#native-source-builds). See [docs/SELF_HOSTING.md](docs/SELF_HOSTING.md) for the audited graph,
verification scope and remaining blockers.

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

`tests/run` holds 495 programs that both rustc and flux compile and run.

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

flux builds with stable Rust. Native executable linking supports arm64 macOS
and x86-64 Linux. Linux needs an installed glibc C toolchain: `cc` locates the
startup objects and shared libraries, and Keel performs the link in-process.

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
