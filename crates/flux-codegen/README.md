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

Native x86-64 Linux linking uses Keel's ELF linker. `cc` must locate the installed
`crt1.o`, `crti.o`, `crtbegin.o`, `crtend.o`, `crtn.o`, `libgcc.a`, glibc, libm, libpthread
and ELF loader. The embedded runtime uses `gettid` (glibc 2.30 or newer).
The driver never invokes a system linker. Requested dynamic libraries use the
same lookup; GNU library scripts are rejected by Keel, so libc and libm use
versioned shared objects. Linux output is a dynamically linked non-PIE executable.
Failed links preserve an existing destination. Linux cross-linking from macOS
and x86-64 macOS (including Rosetta) are not supported.
