# flux-solve

The trait solver of [flux](../../README.md): whether a predicate holds
outside of bodies, given the bounds in scope; what associated types
normalize to; which function a call of a trait's method calls once its
types are known; and the queries over signatures and the item tree that
the type checker shares with it (field types, sizedness, trait objects'
supertraits). It depends on `flux-ty-ir` and not on `flux-ty`, so it
cannot reach into inference. See the
[architecture guide](../../docs/ARCHITECTURE.md).
