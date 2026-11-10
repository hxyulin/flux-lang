# Panic unwinding

Flux defaults to `panic="abort"`. In that mode, `catch_unwind` cannot catch a
panic and ordinary code observes `thread::panicking()` as false. Hooks run
before abort and observe the active panic state. On arm64 macOS, `fluxc build`
and `fluxc run` accept experimental `-C panic=unwind`.

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

`build_file_with_panic_strategy` builds source functions, closure bodies, call
adapters and embedded-library templates with one strategy. The in-process library
MIR caches are separate for abort and unwind, so building either mode does not
replace or reuse the other's checked bodies. Constant initializers still use
abort semantics. Const checking ignores cleanup-only destruction, which cannot
run during constant evaluation.

`mono::instance_body` preserves exceptional control flow through substitution,
expands instruction-like intrinsics without exceptional edges, and retains
exceptional edges for calls and drops. Concrete `read`, `write` and
`drop_in_place` library wrappers are covered by integration tests. Consuming
closure adapters destroy their owned captures on a failed call; borrowed adapters
propagate the exception without destroying the caller's captures.

Coroutine lifting preserves exceptional targets and forwards a panicking await
through the source cleanup graph. Poll failures poison the frame before resuming.
Cancellation uses separate normal and exceptional deallocation paths, so freeing
the frame does not swallow the exception. Completed and poisoned frames only
free their allocation when dropped. Repeated polls preserve that inactive state
and panic with messages checked against native rustc. Mutable references to an
awaited future are reborrowed on each poll.

MIR tests cover eight coroutine factories, including empty bodies, suspension,
partial moves and owned/String outputs. They check that normal and exceptional
cancellation exits free exactly once and never access the frame afterward.
Function-item/pointer and formatting adapters propagate exceptions. Closure
cloning uses temporaries and cleans up completed clones in reverse order if a
later clone fails; a native rustc control checks that order.

MIR distinguishes a call's machine convention from its unwind contract. Calls
through `C` and `system` declarations or pointers have no exceptional successor.
`C-unwind` and `system-unwind` calls retain cleanup. A panic originating inside a
non-unwinding foreign function cleans its locals, then terminates at the boundary.
Closures and coroutine bodies retain Rust's contract even if a foreign function
constructs them. Native rustc controls check cleanup and termination for both
foreign conventions.

Normal CLI compilation still uses abort mode. A compilation retains its explicit
panic strategy even when a source body has no exceptional edges. Unwind metadata
carries a strategy tag; imports reject a different strategy before copying MIR.
Abort MIR metadata retains its existing encoding. Target `cfg(panic)` follows the
selected strategy. Separate builtin variants retain the corresponding MIR and
object code; staging and loading the unwind variant still need qualification.

## Native lowering

The codegen API accepts unwind MIR and lowers exceptional calls to Keel invoke
instructions. Separate landing-pad blocks store the exception pointer before
entering shared MIR cleanup. Resume calls `_Unwind_Resume`; terminating cleanup
uses a catch-all pad so destructors run even without an outer handler. Indirect
calls use cached typed forwarding functions because the pinned Keel lacks an
indirect invoke instruction. Trait-object drops use this path too.

`compile_object_with_panic_strategy` retains unwind tables in all frames for
`Unwind`, including forwarding functions without cleanup. It currently accepts
only arm64 macOS. Ordinary object entry points retain abort mode and reject
Flux exception personalities. The matching library MIR supplies the Flux runtime. Low-level link callers must
include the platform C++ personality library and compatible dependency objects.
The CLI adds the personality library for its experimental unwind mode.

A native test compiles the same source with rustc and Flux at both optimization
levels, then links each object to a rustc panic/catch driver. It checks direct,
indirect, trait, borrowed closure, boxed FnOnce, async poll, aggregate memory
return and floating-point callback paths. Owned String payloads and destructor
events agree. Allocator probes on the Flux objects check exactly-once release
for the exercised Box and coroutine allocations, including a boxed trait-object
destructor that panics. C boundaries terminate after local cleanup. A second
escaping panic finishes the current whole value before terminating.

That test checks native Flux cleanup with a rustc panic runtime. A separate
native comparison now runs programs entirely compiled by Flux, using its own
panic runtime at both optimization levels.

## Panic runtime

The runtime owns `Box<dyn Any + Send>` payloads in a native aligned exception
allocation. A foreign exception class distinguishes its headers. The platform
C++ personality interprets Keel's landing pads; Flux catches its own class and
aborts if a Rust catch boundary receives another runtime's exception.
`catch_unwind` consumes its callback once and transfers either its return value
or its panic payload. `resume_unwind` preserves that allocation and skips hooks.
Deleting a caught exception frees its header without destroying the transferred
payload. Owned `Box<dyn Any + Send>::downcast` also preserves allocation identity.

A POSIX thread key stores the panic depth and hook/preparation state without an
owned TLS allocation. A global active-panic counter avoids TLS syscalls during
ordinary lock queries. Nested catches during destruction restore the outer
panic depth. Panics while preparing a formatted message or running a hook abort.
Hooks can be replaced and taken; replaced hooks are dropped outside the lock.
The common panic path runs hooks under both abort and unwind strategies.

Thread roots catch before returning through pthread's C callback. Joins return
the owned payload, while scopes join every worker before reporting an unjoined
panic. A scope already unwinding preserves the parent's original payload. Mutex
and write-lock guards use the panic state for poisoning. The generated process
root catches an uncaught panic, runs cleanup, flushes stdout and returns status
101. Default abort builds retain no exception personality or unwind catch root.

Native paired tests cover owned literal/formatted/custom payloads, allocation
identity through resume, nested catch in a destructor, payload destruction,
hook replacement, mutex poisoning, thread and scoped-thread results, and a
parent querying its state while another thread unwinds. Fatal cases cover an
uncaught main panic, a hook panic, formatting failure and a double panic. A
separate abort-mode comparison checks hooks and termination without linking the
C++ personality library.

The full goal is not complete. Cargo profiles, full artifact and builtin
qualification, upstream Rayon unwind tests, and full native qualification of
partial initialization, clone failures and suspended future cancellation remain.

Isolated arm64 macOS probes on the pinned Keel revision exercise a direct-invoke
forwarding function containing an indirect call. With unwind tables retained in
all frames, a rustc panic crosses that function, runs the Keel cleanup and reaches
rustc's catch with its owned payload intact. A separate custom-exception probe
uses the platform C++ personality for cleanup/resume and a catch-all landing pad;
the original exception pointer and payload survive and deletion runs once. These
probes qualify mechanisms, not Flux's runtime or its complete indirect-call ABI.

## Next implementation steps

1. Carry the panic strategy through CLI/Cargo profiles, target cfg, builtin
   selection and artifact identities. Preserve the current default abort mode.
2. Qualify all partial-initialization, assertion, clone-failure and suspended
   future paths with the Flux runtime, including exact deallocation checks.
3. Enable and pass the upstream Rayon unwind/panic-safety tests, then run complete
   validation on the final integration.

The first backend target is arm64 macOS. The pinned Keel supports native exception
cleanup there; its AArch64 Linux and x86-64 Linux backends reject exception-handling
personalities. A platform unwinder alone cannot provide Rust destructor or panic
payload semantics.
