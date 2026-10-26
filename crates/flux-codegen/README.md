# flux-codegen

Code generation for [flux](../../README.md) through
[keel](https://github.com/hxyulin/keel), in-process: MIR to keel IR, keel
IR to a native object, and the object to an executable with keel's
linker. See the [architecture guide](../../docs/ARCHITECTURE.md).

`compile_object`, `compile_object_with_timings` and `link_executable` return
`flux_codegen::Error`, rather than a string. Inspect `stage()` to distinguish
verification, optimization, codegen, assembly, SDK, linking and output failures.
`Display` retains the diagnostic text; `std::error::Error::source` retains causes
where the backend supplies an error type. Keel optimization errors remain stored
as structured values; its linker currently supplies only text.
