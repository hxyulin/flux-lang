# flux-ty-ir

The type representation of [flux](../../README.md) that the trait solver
(`flux-solve`) and the type checker (`flux-ty`) share: interned types,
predicates and projections, each item's resolved signature
(`Signatures`), and the region tables the borrow checker reads beside
them. It holds data and pure functions over it; resolving types is
`flux-ty`'s and solving is `flux-solve`'s. See the
[architecture guide](../../docs/ARCHITECTURE.md).
