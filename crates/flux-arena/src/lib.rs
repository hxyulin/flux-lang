//! Arena allocation for the flux compiler.
//!
//! A compiler allocates many small objects whose lifetimes end together:
//! the strings of a symbol table, the nodes of one function's IR. Arenas
//! make each allocation a pointer bump or a vector push and free everything
//! at once.
//!
//! - [`DroplessArena`] bump-allocates `Copy` values, slices and strings and
//!   hands out references that live as long as the arena.
//! - [`Arena`] stores values of one type in a vector and names them by
//!   [`Idx`], a 32-bit index. Indices are `Copy`, need no lifetimes, are
//!   half the size of a pointer, and stay meaningful when the arena is sent
//!   to another thread or serialized, which suits IRs built in parallel or
//!   cached between compilations. [`ArenaMap`] attaches extra data to the
//!   values of an arena without changing it.
//! - [`LayeredArena`] is an `Arena` whose first values are frozen and
//!   shared by its copies, so many arenas can continue one large one
//!   (every program's items continue `core`'s) without copying it.
//!   [`LayeredArenaMap`] shares frozen side tables with private overrides.
//! - [`SlicePool`] stores child lists in one vector, named by 32-bit
//!   [`SliceId`] ranges. [`SharedSlicePool`] batches immutable `Copy`
//!   slices into reference-counted chunks when slices must own their storage.
//! - [`StaticSliceArena`] batches process-lifetime interned `Copy` slices.

mod dropless;
mod shared_slice;
mod slice;
mod static_slice;
mod typed;

pub use dropless::DroplessArena;
pub use shared_slice::{SharedSlice, SharedSlicePool};
pub use slice::{SliceId, SlicePool};
pub use static_slice::StaticSliceArena;
pub use typed::{Arena, ArenaMap, Idx, LayeredArena, LayeredArenaMap};
