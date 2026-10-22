//! A fixed-capacity hash map with open addressing (linear probing and
//! tombstones), generic over the key, the value and the hasher. Hashing is
//! split the way `core::hash` splits it: a `Hash` trait for keys that feed
//! bytes to a `Hasher`, and a `BuildHasher` with an associated type. Two
//! hashers (FNV-1a and a deliberately weak one that collides a lot) are
//! compared on a word-frequency count over a text, with insert, get,
//! get_mut, an entry-style `get_or_insert_with`, remove and iteration.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

trait Hasher {
    fn write(&mut self, bytes: &[u8]);
    fn finish(&self) -> u64;

    fn write_u8(&mut self, b: u8) {
        self.write(&[b]);
    }
    fn write_u32(&mut self, x: u32) {
        self.write(&x.to_le_bytes());
    }
}

trait BuildHasher {
    type Hasher: Hasher;
    fn build_hasher(&self) -> Self::Hasher;

    fn hash_one<K: Hash + ?Sized>(&self, key: &K) -> u64 {
        let mut h = self.build_hasher();
        key.hash(&mut h);
        h.finish()
    }
}

trait Hash {
    fn hash<H: Hasher>(&self, state: &mut H);
}

impl Hash for str {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.as_bytes());
        state.write_u8(0xff);
    }
}

impl Hash for u32 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u32(*self);
    }
}

impl<T: Hash + ?Sized> Hash for &T {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

struct Fnv1a(u64);

impl Hasher for Fnv1a {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x100_0000_01b3);
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Default)]
struct FnvBuilder;

impl BuildHasher for FnvBuilder {
    type Hasher = Fnv1a;
    fn build_hasher(&self) -> Fnv1a {
        Fnv1a(0xcbf2_9ce4_8422_2325)
    }
}

/// Sums the bytes: anagrams collide, and so do many short words.
struct ByteSum(u64);

impl Hasher for ByteSum {
    fn write(&mut self, bytes: &[u8]) {
        self.0 = bytes.iter().fold(self.0, |acc, &b| acc + b as u64);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Default)]
struct WeakBuilder;

impl BuildHasher for WeakBuilder {
    type Hasher = ByteSum;
    fn build_hasher(&self) -> ByteSum {
        ByteSum(0)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Slot<K, V> {
    Empty,
    Tombstone,
    Full(K, V),
}

struct HashMap<K, V, S, const N: usize> {
    slots: [Slot<K, V>; N],
    len: usize,
    builder: S,
    probes: u64,
}

impl<K, V, S, const N: usize> HashMap<K, V, S, N>
where
    K: Hash + Eq + Copy,
    V: Copy,
    S: BuildHasher + Default,
{
    const CAPACITY: usize = N;

    fn new() -> Self {
        HashMap { slots: [Slot::Empty; N], len: 0, builder: S::default(), probes: 0 }
    }

    fn len(&self) -> usize {
        self.len
    }

    /// The slot holding `key`, or `Err` with the first free slot to use.
    fn find(&mut self, key: &K) -> Result<usize, Option<usize>> {
        let start = (self.builder.hash_one(key) % N as u64) as usize;
        let mut first_free = None;
        for i in 0..N {
            let idx = (start + i) % N;
            self.probes += 1;
            match &self.slots[idx] {
                Slot::Empty => return Err(first_free.or(Some(idx))),
                Slot::Tombstone => {
                    first_free.get_or_insert(idx);
                }
                Slot::Full(k, _) if k == key => return Ok(idx),
                Slot::Full(..) => {}
            }
        }
        Err(first_free)
    }

    fn insert(&mut self, key: K, value: V) -> Option<V> {
        match self.find(&key) {
            Ok(idx) => {
                let old = core::mem::replace(&mut self.slots[idx], Slot::Full(key, value));
                match old {
                    Slot::Full(_, v) => Some(v),
                    _ => unreachable!(),
                }
            }
            Err(Some(idx)) => {
                self.slots[idx] = Slot::Full(key, value);
                self.len += 1;
                None
            }
            Err(None) => panic!("map is full"),
        }
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        let idx = self.find(key).ok()?;
        match &self.slots[idx] {
            Slot::Full(_, v) => Some(v),
            _ => None,
        }
    }

    fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let idx = self.find(key).ok()?;
        if let Slot::Full(_, v) = &mut self.slots[idx] { Some(v) } else { None }
    }

    fn get_or_insert_with<F: FnOnce() -> V>(&mut self, key: K, make: F) -> &mut V {
        let idx = match self.find(&key) {
            Ok(idx) => idx,
            Err(free) => {
                let idx = free.expect("map is full");
                self.slots[idx] = Slot::Full(key, make());
                self.len += 1;
                idx
            }
        };
        match &mut self.slots[idx] {
            Slot::Full(_, v) => v,
            _ => unreachable!(),
        }
    }

    fn remove(&mut self, key: &K) -> Option<V> {
        let idx = self.find(key).ok()?;
        self.len -= 1;
        match core::mem::replace(&mut self.slots[idx], Slot::Tombstone) {
            Slot::Full(_, v) => Some(v),
            _ => None,
        }
    }

    fn iter(&self) -> impl Iterator<Item = (&K, &V)> + '_ {
        self.slots.iter().filter_map(|s| match s {
            Slot::Full(k, v) => Some((k, v)),
            _ => None,
        })
    }

    fn load_percent(&self) -> usize {
        self.len * 100 / Self::CAPACITY
    }
}

const TEXT: &str = "the quick brown fox jumps over the lazy dog the dog barks \
    and the fox runs a stack is not a heap and a heap is not a stack \
    tops spot pots stop opts post was saw on no deed dog god act cat";

fn count_words<S: BuildHasher + Default>(label: &str) -> u64 {
    let mut counts: HashMap<&str, u32, S, 64> = HashMap::new();
    for word in TEXT.split_whitespace() {
        *counts.get_or_insert_with(word, || 0) += 1;
    }
    let distinct = counts.len();
    let total: u32 = counts.iter().map(|(_, &c)| c).sum();
    let (top_word, top_count) = counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(w, c)| (*w, *c))
        .unwrap();
    unsafe {
        printf(
            c"%-6.*s distinct %zu total %u top '%.*s'x%u load %zu%%\n".as_ptr(),
            label.len() as i32,
            label.as_ptr(),
            distinct,
            total,
            top_word.len() as i32,
            top_word.as_ptr(),
            top_count,
            counts.load_percent(),
        );
    }

    // Remove every word that occurs once, then check lookups still work
    // across the tombstones they leave.
    let mut singles = [""; 64];
    let mut n = 0;
    for (w, &c) in counts.iter() {
        if c == 1 {
            singles[n] = *w;
            n += 1;
        }
    }
    for w in &singles[..n] {
        assert_eq!(counts.remove(w), Some(1));
    }
    assert!(counts.get(&"fox").is_some_and(|&c| c == 2));
    assert_eq!(counts.get(&"tops"), None);
    if let Some(c) = counts.get_mut(&"the") {
        *c *= 10;
    }
    let the = counts.get(&"the").copied().unwrap_or(0);
    counts.insert("tops", 7);
    let old = counts.insert("tops", 8);
    unsafe {
        printf(
            c"       after removing %d singles: len %zu, the=%u, reinsert old=%d, probes %llu\n".as_ptr(),
            n as i32,
            counts.len(),
            the,
            old.map_or(-1, |v| v as i32),
            counts.probes,
        );
    }
    counts.probes
}

fn main() {
    let fnv = FnvBuilder;
    let h1 = fnv.hash_one("hello");
    let h2 = fnv.hash_one(&"hello");
    let h3 = fnv.hash_one(&42u32);
    unsafe { printf(c"fnv(hello) = %016llx same=%d, fnv(42) = %016llx\n".as_ptr(), h1, (h1 == h2) as i32, h3) };

    let fast = count_words::<FnvBuilder>("fnv");
    let slow = count_words::<WeakBuilder>("weak");
    unsafe { printf(c"weak hasher probes %llu more\n".as_ptr(), slow - fast) };

    // Integer keys: squares mod a prime, with collisions forced by a small table.
    let mut residues: HashMap<u32, u32, FnvBuilder, 32> = HashMap::new();
    for x in 1..=30u32 {
        let r = x * x % 31;
        let seen = residues.get_or_insert_with(r, || 0);
        *seen += 1;
    }
    let mut keys = [0u32; 32];
    let mut nk = 0;
    for (&k, &v) in residues.iter() {
        assert_eq!(v, 2);
        keys[nk] = k;
        nk += 1;
    }
    keys[..nk].sort_unstable();
    unsafe { printf(c"%zu quadratic residues mod 31:".as_ptr(), nk) };
    for k in &keys[..nk] {
        unsafe { printf(c" %u".as_ptr(), *k) };
    }
    unsafe { printf(c"\n".as_ptr()) };
}
