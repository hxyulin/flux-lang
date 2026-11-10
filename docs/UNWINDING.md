# Panic unwinding

Flux still compiles with `panic="abort"`. `catch_unwind` cannot catch a panic
and `thread::panicking()` still returns false. There is no user-facing
`panic="unwind"` option yet.

## Implemented foundation

MIR terminators carry an exceptional action: unreachable, continue unwinding,
enter a cleanup block, or terminate if an exception escapes. A cleanup can end
with `UnwindResume`. CFG traversal includes cleanup edges. Initialization analysis
moves call arguments before either successor and initializes the destination only
on the normal successor. A diverging call can still reach cleanup.

The MIR validator rejects cleanup reached from ordinary control flow, normal
returns from cleanup, resuming outside cleanup, invalid exceptional targets, and
cleanup that permits a second exception to escape. Fluxlib metadata round-trips
these edges. The existing abort-mode encoding stays byte-identical.

These are IR building blocks. The source builder still produces abort-mode MIR.
Drop elaboration and instance expansion explicitly reject exceptional MIR until
they can preserve cleanup semantics. Code generation reports an unsupported-feature
diagnostic before transforming it. No panic runtime or emitted unwind path is
enabled by this foundation.

## Next implementation steps

1. Build cleanup ladders from lexical scopes before drop elaboration. Reuse move
   paths and drop flags. Clear a value's ownership before its destructor runs,
   preserve the remaining fields if that destructor panics, and abort on another
   panic during cleanup. Cover partially initialized aggregates and indirect calls.
2. Lower exceptional calls and cleanup to Keel invoke and landing-pad instructions.
   Preserve unwind metadata in ordinary forwarding frames. Distinguish `C` and
   `C-unwind` boundaries. The pinned Keel has no indirect invoke instruction.
3. Add a panic runtime over the platform unwinder: owned payloads, personality,
   catch/resume, thread-local panic depth, hooks and double-panic termination.
   Catch at thread roots before returning through pthread's C callback.
4. Add an opt-in panic strategy to configuration, package profiles, built-in
   variants and artifact identity. Qualify join payloads and mutex poisoning
   before enabling the skipped Rayon unwind tests.

The first backend target is arm64 macOS. The pinned Keel supports native exception
cleanup there; its AArch64 Linux and x86-64 Linux backends reject exception-handling
personalities. A platform unwinder alone cannot provide Rust destructor or panic
payload semantics.
