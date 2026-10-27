# Observed readiness

The focused test on 2026-10-25 stopped at HIR in 7.97 seconds.
Syntax had 0 diagnostics; HIR had 138. Type checking, MIR and codegen did
not run. The separately compiled rustc reference passed:

```text
aarch64 macho text=12 relocations=1
x86_64 elf text=11 relocations=1
```

The installed compiler was rustc 1.99.1 (58cf8f9c3 2026-10-13), with host
`aarch64-apple-darwin`. The original scripts emitted object `core_error` and
crc32fast `stable_arm_crc32_intrinsics` and `stable_vpclmulqdq`.
All four input integrity checks were clean, including the parent check.

The final run artifacts are under `out/keel-asm-readiness/62964/`.
`configuration.json` records the exact 13-package input graph with package
IDs, dependency indices, editions, features, source paths and rustc artifact
identities. The consumer is index 13, edition 2024, with direct dependencies
12 (keel-asm) and 11 (object), as listed in README.md. `metadata.json` retains
the locked offline metadata. `source-inventory.json` retains 521 original
files and 9,899,137 original bytes. `consumer.rs` retains the consumer input.
Rustc outputs and the runnable reference are in `oracle/`.

The first HIR diagnostic is cfg-if 1.0.5 `src/lib.rs:66`, byte range
`2089..2090`: `expected identifier, found $` (the diagnostic quotes `$` with
backticks). The next is crc32fast 1.5.3 `src/lib.rs:112`, byte range
`3414..3419`: `cannot find State in specialized`.
Full paths, byte ranges and messages for all 138 diagnostics are in
`hir.diagnostics`. The counts below describe diagnostics, including cascades;
they do not claim 138 independent defects or assign compiler/library work.

| Diagnostic | Count |
| --- | ---: |
| expected identifier, found `$` | 1 |
| cannot find `State` in `specialized` | 1 |
| the `test` attribute is not supported yet | 4 |
| impl Trait in edition 2021 are not supported yet | 4 |
| `self` parameters with a type are not supported yet | 6 |
| cannot find value `self` in this scope | 6 |
| lifetime bounds are not supported yet | 2 |
| transparent structs with multiple or no fields are not supported yet | 6 |
| unresolved import `std::io::Read` | 1 |
| unresolved import `std::io::Seek` | 1 |
| unresolved import `std::io::SeekFrom` | 1 |
| `as _` imports are not supported yet | 1 |
| generic associated types are not supported yet | 70 |
| impl Trait in edition 2018 are not supported yet | 5 |
| `where` clauses on associated types are not supported yet | 10 |
| unresolved import `std::collections::BTreeSet` | 3 |
| cannot find `BTreeSet` in `collections` | 1 |
| unresolved import `std::arch` | 1 |
| cannot find function `vld1q_u8` in this scope | 1 |
| cannot find function `vceqq_u8` in this scope | 2 |
| cannot find function `vdupq_n_u8` in this scope | 2 |
| cannot find function `vorrq_u8` in this scope | 1 |
| cannot find function `vget_lane_u64` in this scope | 1 |
| cannot find function `vreinterpret_u64_u8` in this scope | 1 |
| cannot find function `vshrn_n_u16` in this scope | 1 |
| cannot find function `vreinterpretq_u16_u8` in this scope | 1 |
| cannot find `time` in `std` | 4 |

The missing APIs include `std::io::{Read, Seek, SeekFrom}` in object,
`std::collections::BTreeSet`, `std::arch::aarch64` SIMD intrinsics, and
`std::time` in keel-asm. The build scripts and enabled source features remain
unchanged. The test fails at the first dirty stage and does not bypass these
blockers to reach codegen.

Validation command: `cargo test -p flux-cli --test keel_asm_readiness -- --ignored --nocapture`
(run through the installed local mbx wrapper). No formatting, full workspace
validation, compiler/library edits, commits or public actions were performed
for this harness.

This remains an opt-in progress audit while the required language and library
features are outside the supported subset. It is ignored by the ordinary regression
suite; explicit execution still fails at the first dirty compiler stage. The
refactor validation rerun reported 152 HIR diagnostics with unchanged source bytes,
including GATs, CRC feature detection/intrinsics and missing std APIs. No native
assembler milestone is claimed. The implemented asm-ir and Keel IR graphs have
separate active regression tests.

## Resumed audit on 2026-10-27

The guarded audit in `out/keel-asm-readiness/49032` ran after adding typed
receivers, anonymous imports, Read/Seek traits and monotonic time APIs.
Syntax has 0 diagnostics and HIR has 132, down from the preceding 152-diagnostic
inventory. Both worker and parent source-integrity reports are `[]`. The rustc
consumer passed. Types, MIR and native Flux codegen did not run.

The remaining diagnostics are 80 GAT/associated-where gates, 20 CRC feature or
intrinsic errors, 11 architecture/SIMD names, 9 pre-2024 return-position
`impl Trait` gates, 6 multi-field transparent wrappers, 4 test attributes and
2 lifetime bounds. These are diagnostic counts, including possible cascades.
There are no remaining typed-receiver, anonymous-import, Read/Seek or time
name-resolution errors in this inventory.

The ignored parent test now passes `--ignored` to its worker and requires an
inventory artifact. Without those checks an explicitly invoked parent could
return success after its worker skipped the audit.
