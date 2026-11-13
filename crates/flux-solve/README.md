# flux-solve

The trait solver of [flux](../../README.md): whether a predicate holds
outside of bodies, given the bounds in scope; what associated types
normalize to; which function a call of a trait's method calls once its
types are known; and the queries over signatures and the item tree that
the type checker shares with it (field types, sizedness, trait objects'
supertraits). It depends on `flux-ty-ir` and not on `flux-ty`, so it
cannot reach into inference. See the
[architecture guide](../../docs/ARCHITECTURE.md).

Caller-bound matching lives in `src/traits/caller.rs`. It compares facts and
requested predicates under the same normalization, while preserving three
proof contexts: original memberships, declaration inputs, and value uses.
The caller chooses the available normalization rules. Matching never adds an
impl candidate to the caller's assumptions. The rigid solver still requires
independent trait evidence before it uses a conditional associated output.

Inference-time candidate probing remains in `flux-ty`. Recursion limits,
coherence, and sizedness checks remain separate from caller-bound matching.
