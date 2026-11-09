//! Collections with the caller-supplied hashing used by compiler source graphs.
pub use crate::alloc_crate::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, TryReserveError};
pub use crate::alloc_crate::collections::btree_map;
pub use crate::alloc_crate::collections::binary_heap;
pub mod hash_map;
pub mod hash_set;
pub mod vec_deque;
pub use self::hash_map::HashMap;
pub use self::hash_set::HashSet;
pub use self::vec_deque::VecDeque;
