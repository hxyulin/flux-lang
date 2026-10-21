# flux-arena

Arena allocation for [flux](../../README.md).

- `DroplessArena`: bump allocation of `Copy` values, slices and strings,
  freed all at once.
- `Arena<T>` and `Idx<T>`: values of one type named by typed 32-bit
  indices, for IRs. `ArenaMap` attaches data to an arena's values.

The `unsafe` code here is tested under Miri (`just miri`).
