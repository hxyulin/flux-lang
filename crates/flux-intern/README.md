# flux-intern

Global, thread-safe string interning for [flux](../../README.md).

```rust
use flux_intern::{Symbol, kw};
let main = Symbol::intern("main");
assert_eq!(main.as_str(), "main");
assert_eq!(Symbol::intern("fn"), kw::Fn);
```

A `Symbol` is 4 bytes and compares as an integer. Interning goes through a
per-thread cache and a sharded table; resolving takes no lock. See the
crate documentation for the design.
