# flux-mir

The mid-level IR of [flux](../../README.md): each function as a
control-flow graph of basic blocks over locals, built from typed HIR,
with Rust's runtime semantics made explicit (wrapping arithmetic, masked
shifts, division checks, short-circuiting). Also checks that locals are
initialized before use. See the
[architecture guide](../../docs/ARCHITECTURE.md).
