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

Full optimization uses Keel's Full instruction selector and allocator after IR
optimization. Other tiers retain baseline instruction selection. This enables
target address folding and excludes encoded operands from register liveness in
optimized objects.

Lowering caches normalized function signatures by resolved instance for one
compilation. Definitions and direct calls share this cache; ABI conversion still runs
for each calling convention and `track_caller` wrapper.

Whole-program lowering gives private generic instances names from a module-local
ordinal. These names need no type serialization or content hash. Separate objects
keep stable names derived from definition identities and generic arguments so
instances can coalesce across objects.

When the backend exposes `compact-unused-functions`, Flux runs it after each
Full inlining round and at the pipeline's final cleanup point. It removes stale
callee declarations and unreachable private Fast bodies before later passes.
Other tiers use it at their existing cleanup point. Backends without the pass
retain their standard schedule.

Set `FLUX_KEEL_PROFILE` to an output directory to save Keel's optimizer report
and the input IR for each object compilation. Filenames include the process ID
and a sequence number. The JSON records pass and function timings, IR counts,
inlining decisions and verification costs. The `.input.kl` file can be replayed
with Keel's CLI. Partial reports survive optimizer errors.

Use a release compiler for throughput comparisons. Checked stage0 enables
Keel's repeated verification and fingerprint audits. Profiling also counts IR
and writes potentially large files, so measure ordinary compilation separately
with the variable unset.

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
