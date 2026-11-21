# Keel assembler readiness

Run `cargo test -p flux-cli --test keel_asm_readiness -- --nocapture` on
AArch64 macOS. The test keeps build logs and source integrity reports in
`out/keel-asm-readiness/<parent-pid>/`.

The harness takes the Keel revision and dependency versions from the workspace
lockfile. It builds the same assembler consumer with Cargo and rustc, then
with Flux in debug and release modes. Flux uses rustc for host proc macros
and compiles the target libraries from source. The harness verifies that
all dependency package IDs came from the workspace and compares the saved
inputs before and after the builds.

The consumer assembles AArch64 Mach-O and x86-64 ELF. It checks each object's
architecture, sections, text bytes, relocations, and symbols. The three
consumer executables must agree on exit status, stdout, and stderr.
