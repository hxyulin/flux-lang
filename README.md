# flux

An experimental compiler for a subset of Rust, built to learn how Rust's
toolchain works, measure where its costs come from, and eventually design a
language of its own. It will also stress-test the
[keel](https://github.com/hxyulin/keel) compiler backend with Rust-shaped
code: monomorphized generics, small aggregates passed by value, enums and
heavy inlining.

flux accepts only programs that are also valid Rust (edition 2024), so
`rustc` can serve as a test oracle: a program flux compiles must behave the
same under both.

## Status

flux compiles its first programs: modules and functions over integers,
`bool`, tuples and structs, with methods, generics (monomorphized),
traits (statically dispatched), `Copy`, `Clone` and `Drop`, references checked by a borrow checker, constants evaluated at compile
time, C strings, and calls to C through `unsafe extern "C"` (including
`printf`), down to native arm64 macOS executables through keel. flux has
its own small `core` library, written in flux. Programs in
[`tests/run`](tests/run) are built by both rustc and flux and must behave
identically. Everything else in Rust is on the [roadmap](docs/ROADMAP.md):
associated constants and bounds on structs come next, then enums, so
that `core` can grow.

The parser covers much more of Rust than the later stages: it handles
all of its own sources and keel's, and parses 1 GiB of crates.io sources
without a panic and with every tree reproducing its file exactly. Most of the syntax errors there are deliberate: older editions'
keywords and features outside the [supported subset](docs/LANGUAGE.md).

## Try it

```sh
cargo run -p flux-cli -- run tests/run/fib.rs      # compile and run a program
cargo run -p flux-cli -- build tests/run/fib.rs    # write an executable, ./fib
cargo run -p flux-cli -- mir tests/run/fib.rs      # print its MIR
cargo run -p flux-cli -- parse examples/hello.rs   # print the syntax tree
cargo run -p flux-cli -- lex examples/hello.rs     # print the tokens
cargo run -p flux-cli -- check examples/*.rs        # report syntax errors
```

## Documentation

- [Architecture](docs/ARCHITECTURE.md): the pipeline and its data structures.
- [Supported language](docs/LANGUAGE.md): which Rust syntax flux accepts.
- [Roadmap](docs/ROADMAP.md): what comes next, including incremental checking.
- [Contributing](CONTRIBUTING.md): setup, tests and conventions.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
