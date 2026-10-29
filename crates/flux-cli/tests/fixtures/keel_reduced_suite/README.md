# Flux-compiled Keel reduced suite

Run on arm64 macOS:

```sh
cargo test -p flux-cli --test keel_reduced_suite -- --ignored --nocapture
```

The suite compiles the same complete pinned backend/linker dependency graph as
`keel_backend_readiness`. Target libraries are unchanged and compiled by Flux.
Cargo and rustc provide host build scripts/proc macros and the reference build.

`selection.json` selects 19 actual upstream test functions: eleven SSA-builder
cases, three rewrite-verifier rejection cases, two DCE/memory-effect cases, and
three call-operand ABI cases. The harness parses the pinned source and copies
only the named functions plus their selected helpers and imports. Function
bodies and assertions remain byte-for-byte unchanged; only `#[test]` registration
is removed so an ordinary consumer can call them. It records physical paths,
ranges, adaptations and original bytes in the audit artifacts. Production source
is never rewritten. This is a selected suite, not the entire upstream test suite.

The extra driver in `consumer.rs.txt` constructs five executable semantic cases:
a counted loop/branch, direct calls, stack loads/stores, signed division/remainder,
and an indirect function-pointer call/relocation. Each runs with Keel optimization
None and Full. The resulting AArch64 programs return zero only when their computed
result matches the expected value. The host harness executes all ten outputs
from the rustc-built reference and both Flux-built backend modes. X86 objects are
also assembled and inspected, but not executed on the arm64 host.

The backend uses Keel's own assembler, linker and ad-hoc signing writer. The
macOS SDK supplies libSystem stubs. Separate byte guards cover original source,
selected upstream test files, generated adapters, embedded library, manifests,
lock file, graph configuration, helper and driver. Artifacts retain diagnostics,
phase markers, timeout statuses, outputs and each generated program's exit status.

## Observed validation

`out/keel-reduced-suite/49440` completed in 203.21 seconds with phase
`full-native-verified`. Input, syntax, HIR, type and full MIR reports are zero;
all five byte-integrity guards are `[]`. Both Flux-built backend modes matched
the rustc-built consumer's stdout, stderr and status.

Each of the three producers (rustc reference, Flux unoptimized, Flux optimized)
ran the 19 selected upstream cases and generated ten AArch64 executables. All
30 generated executables were executed, returned status zero and produced no
unexpected output. The corresponding x86 objects were inspected, not executed.
No production Keel sources, backend pin or compiler/library implementation was
changed for this proof.
