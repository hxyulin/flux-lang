# Panic unwinding

Flux still compiles with `panic="abort"`. `catch_unwind` cannot catch a panic
and `thread::panicking()` still returns false. There is no user-facing
`panic="unwind"` option yet.

## Implemented foundation

MIR terminators carry an exceptional action: unreachable, continue unwinding,
enter a cleanup block, or terminate if an exception escapes. A cleanup can end
with `UnwindResume` or `UnwindTerminate`. CFG traversal includes cleanup edges. Initialization analysis
moves call arguments before either successor and initializes the destination only
on the normal successor. A diverging call can still reach cleanup.

The MIR validator rejects cleanup reached from ordinary control flow, normal
returns from cleanup, resuming outside cleanup, invalid exceptional targets, and
cleanup that permits a second exception to escape. Fluxlib metadata round-trips
these edges. The existing abort-mode encoding stays byte-identical.

`build_with_panic_strategy(..., PanicStrategy::Unwind)` builds a function's
lexical cleanup ladders before drop elaboration. Cleanup suffixes are shared.
Moved values are removed. Conditional drop flags are captured before ownership
is cleared, so the remaining split fields still get dropped if an earlier field
panics. An exception escaping a terminating cleanup boundary aborts. The return slot is excluded
from lexical cleanup, matching rustc's behavior when an epilogue panics.

A test executes the generated MIR and compares drop events with native rustc
under `panic=unwind` in 25 cases. These include direct and indirect calls,
conditional initialization, partial tuple/array/enum moves, assignment, call
arguments, failed aggregate construction, assertions, epilogue panics and double
panics. The test also checks metadata round-trips of those graphs. This tests MIR
cleanup behavior; it does not execute Flux-generated native unwind paths.

Whole-value drop expansion preserves each child's exception boundary. If a
user destructor panics, the remaining fields are cleaned up. Array and slice
loops continue from the current element, including its remaining fields. Packed
fields move into aligned temporaries before their destructors run. Non-dynamic
`drop_in_place` intrinsics use the same expansion.

A panic during cleanup can still finish the current value before termination.
`UnwindTerminate` represents that final exit. The validator permits nested
cleanup only when all of its exits terminate rather than resume. Specialization
that removes all drops also removes unreachable exceptional exits.

The expanded-MIR oracle covers 52 cases, including nested destructor failures,
whole structs/enums/tuples, nested arrays, packed fields, slices, and concrete
specializations with and without destructors. Slice tests substitute the private
intrinsic for its public forwarding wrapper to isolate glue expansion. These
checks do not qualify general wrapper instantiation, dynamic drop calls or Box
deallocation during unwinding. Metadata round-trips preserve every tested graph.

Normal source compilation, closure builders and generated shims still use abort
mode. General instance expansion and code generation still reject exceptional
MIR until remaining intrinsics and backend lowering can preserve it. No panic runtime is enabled.

## Next implementation steps

1. Complete general instance expansion and remaining intrinsics. Extend the
   panic strategy through closure bodies and generated shims. Qualify Box
   deallocation and dynamic drop calls during unwinding.
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
