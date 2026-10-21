//! Global, thread-safe string interning.
//!
//! A [`Symbol`] is a 4-byte handle for a string. Interning the same string
//! always gives the same symbol, so comparing and hashing names is integer
//! work, and every distinct name is stored once for the whole process.
//!
//! ```
//! use flux_intern::{Symbol, kw, sym};
//! let a = Symbol::intern("main");
//! assert_eq!(a, Symbol::intern("main"));
//! assert_eq!(a.as_str(), "main");
//! assert_eq!(Symbol::intern("fn"), kw::Fn);
//! assert_eq!(sym::u8.as_str(), "u8");
//! ```
//!
//! # Design
//!
//! The interner is a process-wide static, so symbols from different
//! threads, files and compilations agree, and a symbol resolves to a
//! `&'static str` with no context object to pass around. The cost is that
//! interned strings are never freed, which is the usual trade-off in
//! compilers: the set of distinct names in a program is small.
//!
//! - **Interning** hashes the string once and first checks a small cache
//!   owned by the calling thread. Names follow a steep frequency curve
//!   (`self`, `T`, `new`...), so most lookups end there without touching
//!   shared memory. On a miss, the hash picks one of 32 shards, each a mutex
//!   around a hash table and a [`DroplessArena`] holding that shard's
//!   strings. Without the cache, every thread would queue on the shards of
//!   the most common names.
//! - **Resolving** takes no lock. Symbols are numbered from one atomic
//!   counter and each number's string is published in an append-only table
//!   of atomic pointers, read with acquire ordering.
//! - **Predefined symbols** (keywords, primitive types, common names) are
//!   interned first, in a fixed order, so they are compile-time constants:
//!   [`kw::Fn`], [`sym::u8`].

mod predefined;

use std::cell::Cell;
use std::fmt;
use std::hash::{BuildHasher, Hash};
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock, PoisonError, RwLock};

use flux_arena::DroplessArena;
use hashbrown::HashTable;

pub use predefined::{kw, sym};

/// An interned string. See the [crate documentation](crate).
///
/// Symbols compare by identity. They deliberately do not implement `Ord`:
/// the numbering follows interning order, not the strings, so sort by
/// [`as_str`](Symbol::as_str) when order matters.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Symbol(u32);

impl Symbol {
    /// The symbol for `s`, interning it if needed.
    #[inline]
    pub fn intern(s: &str) -> Symbol {
        let hash = rustc_hash::FxBuildHasher.hash_one(s);
        CACHE.with(|cache| {
            let slot = &cache[hash as usize % CACHE_SLOTS];
            let (cached_hash, symbol) = slot.get();
            if cached_hash == hash && symbol != EMPTY && INTERNER.strings.get(symbol.0) == s {
                return symbol;
            }
            let symbol = INTERNER.intern(s, hash);
            slot.set((hash, symbol));
            symbol
        })
    }

    /// The string this symbol stands for.
    pub fn as_str(self) -> &'static str {
        INTERNER.strings.get(self.0)
    }

    /// The symbol's number: dense, starting at 0, in interning order.
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    const fn predefined(index: u32) -> Symbol {
        Symbol(index)
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Statistics about the global interner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Distinct strings interned, predefined symbols included.
    pub symbols: usize,
    /// Bytes reserved by the shards' arenas for string data.
    pub arena_bytes: usize,
}

pub fn stats() -> Stats {
    let arena_bytes = INTERNER
        .shards
        .iter()
        .map(|shard| {
            shard
                .0
                .arena
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .capacity_bytes()
        })
        .sum();
    Stats {
        symbols: INTERNER.next.load(Ordering::Relaxed) as usize,
        arena_bytes,
    }
}

const SHARDS: usize = 128;

/// Entries in each thread's cache: 4096 × 16 bytes = 64 KiB per thread.
const CACHE_SLOTS: usize = 4096;
/// Marks an empty cache slot; never a real symbol (see `Interner::intern`).
const EMPTY: Symbol = Symbol(u32::MAX);

thread_local! {
    /// A direct-mapped cache of recently interned strings: `(hash, symbol)`.
    static CACHE: [Cell<(u64, Symbol)>; CACHE_SLOTS] =
        const { [const { Cell::new((0, EMPTY)) }; CACHE_SLOTS] };
}

static INTERNER: LazyLock<Interner> = LazyLock::new(|| {
    let interner = Interner {
        shards: std::array::from_fn(|_| CachePadded(Shard::default())),
        strings: StringTable::new(),
        next: AtomicU32::new(0),
    };
    for (index, s) in predefined::STRINGS.iter().enumerate() {
        let symbol = interner.intern(s, rustc_hash::FxBuildHasher.hash_one(s));
        assert_eq!(
            symbol.0 as usize, index,
            "predefined symbol {s:?} is listed twice"
        );
    }
    interner
});

/// Only ever constructed as the `INTERNER` static, which is never dropped.
/// The `'static` lifetimes below rely on that.
struct Interner {
    shards: [CachePadded<Shard>; SHARDS],
    strings: StringTable,
    next: AtomicU32,
}

#[derive(Default)]
struct Shard {
    /// `(hash, symbol)`; the hash is kept to make resizing cheap. Readers
    /// share the lock; inserting takes it exclusively.
    table: RwLock<HashTable<(u64, Symbol)>>,
    /// The shard's string data. Locked only while inserting, after `table`.
    arena: Mutex<DroplessArena>,
}

/// Keeps each shard's lock on its own cache line (128 bytes on Apple
/// silicon), so threads locking neighboring shards do not slow each other.
#[repr(align(128))]
struct CachePadded<T>(T);

impl Interner {
    /// Interns `s`, whose hash is `hash`.
    fn intern(&self, s: &str, hash: u64) -> Symbol {
        // hashbrown uses the low bits of the hash for the bucket and the top
        // seven for its tag, so take the shard from the middle.
        let shard = &self.shards[(hash >> 40) as usize % SHARDS].0;
        let strings = &self.strings;
        let eq = |&(h, symbol): &(u64, Symbol)| h == hash && strings.get(symbol.0) == s;
        // Most calls find the string, so look under a shared lock first.
        {
            let table = shard.table.read().unwrap_or_else(PoisonError::into_inner);
            if let Some(&(_, symbol)) = table.find(hash, eq) {
                return symbol;
            }
        }
        let mut table = shard.table.write().unwrap_or_else(PoisonError::into_inner);
        // Another thread may have interned it between the two locks.
        if let Some(&(_, symbol)) = table.find(hash, eq) {
            return symbol;
        }

        let arena = shard.arena.lock().unwrap_or_else(PoisonError::into_inner);
        let stored: &str = arena.alloc_str(s);
        // SAFETY: The arena belongs to a shard of `INTERNER`, a static that
        // is never dropped, so its chunks are never freed; arena allocations
        // never move. The string therefore lives for the rest of the
        // program.
        let stored: &'static str = unsafe { &*ptr::from_ref(stored) };
        let slot: &&'static str = arena.alloc(stored);
        // SAFETY: As above, for the arena slot holding the string.
        let slot: &'static &'static str = unsafe { &*ptr::from_ref(slot) };

        let index = self.next.fetch_add(1, Ordering::Relaxed);
        assert!(index < u32::MAX, "more than u32::MAX symbols interned");
        self.strings.publish(index, slot);
        let symbol = Symbol(index);
        table.insert_unique(hash, (hash, symbol), |&(h, _)| h);
        symbol
    }
}

/// An append-only map from symbol numbers to strings that readers access
/// without locks. Bucket `b` holds `FIRST << b` slots, allocated the first
/// time a symbol number lands in it, so slots never move.
struct StringTable {
    buckets: [OnceLock<Box<[AtomicPtr<&'static str>]>>; BUCKETS],
}

const FIRST_BITS: u32 = 10;
const FIRST: usize = 1 << FIRST_BITS;
/// Enough buckets for every `u32` symbol number.
const BUCKETS: usize = (u32::BITS + 1 - FIRST_BITS) as usize;

impl StringTable {
    fn new() -> StringTable {
        StringTable {
            buckets: std::array::from_fn(|_| OnceLock::new()),
        }
    }

    /// The bucket and slot of symbol number `index`.
    fn locate(index: u32) -> (usize, usize) {
        let j = index as u64 + FIRST as u64;
        let log = (u64::BITS - 1 - j.leading_zeros()) as usize;
        (log - FIRST_BITS as usize, (j - (1 << log)) as usize)
    }

    fn publish(&self, index: u32, s: &'static &'static str) {
        let (bucket, slot) = StringTable::locate(index);
        let bucket = self.buckets[bucket].get_or_init(|| {
            (0..FIRST << bucket)
                .map(|_| AtomicPtr::new(ptr::null_mut()))
                .collect()
        });
        // Release: a thread that loads this pointer with acquire ordering
        // sees the string's bytes and the slot written before it.
        bucket[slot].store(ptr::from_ref(s).cast_mut(), Ordering::Release);
    }

    fn get(&self, index: u32) -> &'static str {
        let (bucket, slot) = StringTable::locate(index);
        let ptr = self.buckets[bucket]
            .get()
            .map_or(ptr::null_mut(), |b| b[slot].load(Ordering::Acquire));
        assert!(
            !ptr.is_null(),
            "symbol {index} was never interned, or was received without synchronization"
        );
        // SAFETY: Non-null pointers in the table were published by
        // `publish` with release ordering, after the `&'static str` they
        // point to was written; the acquire load above synchronizes with it.
        // The pointee lives in a never-freed arena (see `Interner::intern`).
        unsafe { *ptr }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn interning_is_idempotent() {
        let a = Symbol::intern("flux_test_alpha");
        let b = Symbol::intern("flux_test_beta");
        assert_ne!(a, b);
        assert_eq!(a, Symbol::intern(&String::from("flux_test_alpha")));
        assert_eq!(
            (a.as_str(), b.as_str()),
            ("flux_test_alpha", "flux_test_beta")
        );
        assert_eq!(format!("{a} {a:?}"), "flux_test_alpha \"flux_test_alpha\"");
    }

    #[test]
    fn predefined_symbols_are_constants() {
        assert_eq!(Symbol::intern("fn"), kw::Fn);
        assert_eq!(Symbol::intern("Self"), kw::SelfUpper);
        assert_eq!(Symbol::intern("self"), kw::SelfLower);
        assert_eq!(Symbol::intern(""), kw::Empty);
        assert_eq!(Symbol::intern("Option"), sym::Option);
        assert_eq!(Symbol::intern("Fn"), sym::Fn);
        assert_eq!(sym::no_std.as_str(), "no_std");
        assert_eq!(sym::usize.as_str(), "usize");
        for (i, s) in predefined::STRINGS.iter().enumerate() {
            assert_eq!(Symbol::intern(s).as_u32() as usize, i);
        }
    }

    #[test]
    fn unicode_and_long_strings() {
        // Larger than an arena chunk, so it gets a dedicated one.
        let len = if cfg!(miri) {
            600 * 1024
        } else {
            3 * 1024 * 1024
        };
        let long = "x".repeat(len);
        let a = Symbol::intern("ünïcødé_名前");
        let b = Symbol::intern(&long);
        assert_eq!(a.as_str(), "ünïcødé_名前");
        assert_eq!(b.as_str(), long);
        assert_eq!(b, Symbol::intern(&long));
    }

    #[test]
    fn locate_covers_every_number() {
        assert_eq!(StringTable::locate(0), (0, 0));
        assert_eq!(StringTable::locate(FIRST as u32 - 1), (0, FIRST - 1));
        assert_eq!(StringTable::locate(FIRST as u32), (1, 0));
        assert_eq!(
            StringTable::locate(3 * FIRST as u32 - 1),
            (1, 2 * FIRST - 1)
        );
        assert_eq!(
            StringTable::locate(u32::MAX),
            (BUCKETS - 1, (u32::MAX as usize + FIRST) - (1 << 32))
        );
    }

    #[test]
    fn threads_agree_on_symbols() {
        let threads = if cfg!(miri) { 4 } else { 16 };
        let per_thread = if cfg!(miri) { 50 } else { 5_000 };
        let results: Vec<Vec<(String, Symbol)>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    s.spawn(move || {
                        // Overlapping ranges: every string is interned by
                        // several threads at once.
                        (0..per_thread)
                            .map(|i| {
                                let name = format!(
                                    "flux_test_thread_{}",
                                    (i + t * per_thread / 4) % (2 * per_thread)
                                );
                                let symbol = Symbol::intern(&name);
                                assert_eq!(symbol.as_str(), name);
                                (name, symbol)
                            })
                            .collect()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut seen: HashMap<String, Symbol> = HashMap::new();
        for (name, symbol) in results.into_iter().flatten() {
            assert_eq!(*seen.entry(name).or_insert(symbol), symbol);
        }
        let distinct: std::collections::HashSet<_> = seen.values().collect();
        assert_eq!(distinct.len(), seen.len(), "two strings share a symbol");
    }
}
