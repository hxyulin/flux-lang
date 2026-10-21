# flux-ty

Type inference and checking for [flux](../../README.md): function
signatures from the item tree, then each HIR body on its own, with
integer literal defaulting, `!` coercion, and the checks rustc makes
before borrow checking (operator types, casts, `unsafe` calls,
mutability). See the [architecture guide](../../docs/ARCHITECTURE.md).
