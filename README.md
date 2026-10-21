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

flux is at its first stage: a lexer and an error-recovering parser that
produce lossless syntax trees. Name resolution, type checking, borrow
checking and code generation are on the [roadmap](docs/ROADMAP.md).

The parser handles all of its own sources and keel's, and parses 1 GiB of
crates.io sources without a panic and with every tree reproducing its file
exactly. Most of the syntax errors there are deliberate: older editions'
keywords and features outside the [supported subset](docs/LANGUAGE.md).

## Try it

```sh
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
