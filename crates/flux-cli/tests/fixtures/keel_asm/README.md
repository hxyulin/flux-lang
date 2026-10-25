# Keel assembler readiness

Run `cargo test -p flux-cli --test keel_asm_readiness -- --nocapture` on
AArch64 macOS. The test runs one worker with a 300-second deadline and leaves evidence in
`out/keel-asm-readiness/<parent-pid>/`, including failed runs.

The source graph comes from workspace `cargo metadata --offline --locked`.
The harness checks package IDs, every normal dependency edge, editions and
features against the reviewed closure below. Both Keel packages must have
source revision `f37f0bd48b14993e69f3db0eeb70d5babd7cd3fb`.
A changed closure fails the audit and needs review.

| Index | Package | Edition | Features | Normal dependency indices |
| --- | --- | --- | --- | --- |
| 0 | bumpalo 3.20.3 | 2021 | collections, default | |
| 1 | smallvec 1.16.3 | 2018 | | |
| 2 | keel-asm-ir 0.1.0 | 2024 | | 1 |
| 3 | memchr 2.8.3 | 2021 | alloc, default, std | |
| 4 | cfg-if 1.0.5 | 2018 | | |
| 5 | crc32fast 1.5.3 | 2021 | std | 4 |
| 6 | foldhash 0.1.5 | 2021 | | |
| 7 | hashbrown 0.15.5 | 2021 | default-hasher | 6 |
| 8 | equivalent 1.0.3 | 2015 | | |
| 9 | hashbrown 0.17.1 | 2024 | | |
| 10 | indexmap 2.14.2 | 2024 | default, std | 8, 9 |
| 11 | object 0.36.7 | 2018 | archive, coff, elf, macho, pe, read, read_core, std, unaligned, write, write_core, write_std, xcoff | 5, 7, 10, 3 |
| 12 | keel-asm 0.1.0 | 2024 | | 0, 2, 3, 11, 1 |
| 13 | consumer | 2024 | | 12, 11 |

Each package has a distinct rustc metadata value and output path. Dependency
indices retain the two hashbrown package identities; only the extern name
is shared. Rustc libraries remain in `oracle/` and never enter Flux's graph.

The harness inventories original Rust source bytes throughout each package,
including build scripts, package manifests, the Keel workspace manifest,
root manifest and lockfile, library sources, the source loader, and the
consumer. It compares those bytes before reference compilation, before Flux
loading, before each codegen run, and after success or failure. The parent
also checks them after worker exit, including timeout or abort.

The original object and crc32fast build scripts compile and execute with the
installed rustc. Their command logs, stdout, stderr, compiler version and
emitted cfg names are saved. Version threshold assertions check `core_error`
(object, rustc 1.81), `stable_arm_crc32_intrinsics` (crc32fast, rustc 1.80),
and `stable_vpclmulqdq` (crc32fast, rustc 1.89). Both compilers receive those
same cfg names and the resolved package features.

The consumer assembles AArch64 text, writes Mach-O through both assembly APIs,
and assembles x86 Intel text into ELF. It reads both objects with the actual
object dependency and checks architecture, format, relocatable kind, exact
text bytes, relocations, and defined/undefined symbols. The x86 entry has an
explicit function type so object identifies it as a definition. Neither
assembled instruction stream is executed; the native executable runs the
assembler and object reader.

The rustc reference compiles and runs before Flux. Flux then stops at the
first dirty stage: syntax/HIR, types, or MIR. Codegen requires all preceding
stages to be clean. Once clean, native executables at both optimization tiers
must match the separately compiled reference's status, stdout and stderr.

Read `phase.txt`, `audit.stderr`, stage diagnostics, `configuration.json`,
`metadata.json`, `build-script-cfg.json`, `loaded-files.json`, and the source
integrity reports to distinguish readiness blockers from reference or input
audit failures. `source-inventory.json` stores original bytes, not just hashes.
A stage marked as waiting did not run. A failed readiness test does not
assign compiler or library work to this harness.
