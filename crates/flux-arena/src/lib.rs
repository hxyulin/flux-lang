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

mod dropless;
mod typed;

pub use dropless::DroplessArena;
pub use typed::{Arena, ArenaMap, Idx, LayeredArena};
