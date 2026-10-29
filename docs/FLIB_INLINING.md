# Cross-crate inlining proposal

This is follow-up work, not part of the current Fluxlib implementation.
Every library keeps its `.flib` and owning object. Optimized downstream
compilations may emit local copies of selected non-generic functions for direct
calls. Generic instances retain their shared ODR linkage.

## Eligibility

Record an inline policy on each function: unspecified, hint, always or never.
Preserve the policy through the HIR metadata codec. An explicit `never` excludes
the function. Explicit hints and `always` make a checked MIR body a candidate;
they do not bypass target-feature or optimizer legality checks.

For automatic candidates, start with non-recursive leaf bodies with no calls,
drop calls, inline assembly or address-sensitive frame operations. A proposed
initial size limit is four blocks and 32 MIR statements, counted before
monomorphization. Measure that policy before selecting the final limits.

Eligibility depends only on the callee's MIR, attributes and a versioned fixed
policy. It never depends on the caller, its call count or its optimization
budget. Record or derive the same decision in every downstream crate. Validate
target and frame constraints before using a candidate. Keel may still decline
to inline a legal local copy at an individual call site.

## Symbols and addresses

Keep two codegen roles for an eligible instance: its canonical definition and
its object-local direct-call copy. The defining crate still emits the canonical
strong symbol. An optimized consumer may emit a distinct local symbol and route
direct calls to it. None-tier builds keep ordinary external calls.

Every address-taking operation uses the canonical symbol, including function
pointers in constants, vtables, pointer wrappers and references from copied
bodies. Thus taking the same function's address in two objects gives the same
address. A local copy's own address never escapes. Do not duplicate a canonical
strong symbol or change imported ownership to achieve this.

The local copy can use the existing private fast convention if declaration,
call arguments and return lowering agree. The canonical definition keeps its
fixed cross-object convention. Retain the callee's source identity, spans,
`track_caller` behavior and type identities rather than inventing new HIR IDs.

Dependencies referenced by a copied body still contribute external functions,
concrete generic instances, statics and vtables through the existing root
accounting. Use the imported MIR provider; do not re-infer dependency source.
Bound copy expansion and reject recursive copy graphs deterministically.

## Metadata and invalidation

The current codec retains every owned MIR body that was built. For such bodies,
inline-policy tags add metadata without duplicating the body. If eligibility
requires a previously unbuilt body, the writer must build and retain it. Measure
the added bytes separately for explicit hints and automatic leaf candidates.
Do not claim zero growth for those new bodies.

Current dependency hashes already invalidate downstream artifacts whenever the
dependency artifact changes, including body changes. This remains correct.
Future finer incremental keys must include the selected callee's body hash,
inline policy, eligibility-policy version, target and compiler identity. A
private body used by a downstream copy is an input to that downstream build.

## Validation

Compare None and Full against rustc with dependency source removed. Exercise
direct calls together with function pointers, constant pointers, vtables,
private helpers, generics reached by copied bodies and `track_caller`. Check
canonical pointer equality across objects. Inspect optimized Keel IR to prove
that eligible direct calls inline while the owning definition remains present.
Measure artifact bytes, downstream build time, binary size and native runtime.

Generic instances are hidden `weak_odr` definitions with
`no_semantic_interposition`, which Keel inlines at direct calls. This proposal
does not replace shared generic instances with local copies.
