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

Integer SSE/AVX/AVX-512 wrappers keep vector values in memory at Rust call boundaries
and introduce packed Keel values inside intrinsic calls. Function target-feature
attributes grant local ISA permissions. Assembly uses the union of those
permissions after code generation checks each function against its own set.
Runtime AVX detection checks CPUID and OS-managed XMM/YMM state through XCR0.
AVX-512 detection also checks opmask and upper-ZMM state. The crypto subset
includes lane-local PCLMUL/VPCLMUL at 128/256/512 bits and SHA-256 instructions.
