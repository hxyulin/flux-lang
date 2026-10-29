//! Opaque runtime type identities. Keys are types, never diagnostic spellings.

use std::sync::{Arc, Mutex};

use rustc_hash::FxHashMap;

use crate::Ty;

/// A collision-free table shared by signature snapshots, evaluators and mono.
/// The numeric IDs are private to the compiled graph, not an external ABI.
#[derive(Debug, Clone, Default)]
pub struct TypeIds(pub Arc<Mutex<FxHashMap<Ty, u128>>>);
