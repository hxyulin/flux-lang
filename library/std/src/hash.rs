//! Hash traits, keyed randomized builders and the default streaming hasher.
pub use core::hash::*;

/// A keyed builder. Clones retain their keys. As in Rust's std, each thread
/// draws entropy once and gives each new builder the next first key.
#[derive(Clone)]
pub struct RandomState { keys: [u64; 2] }

thread_local! {
    static KEYS: core::cell::Cell<[u64; 2]> = core::cell::Cell::new(RandomState::entropy());
}

impl RandomState {
    pub fn new() -> Self {
        KEYS.with(|keys| {
            let [k0, k1] = keys.get();
            keys.set([k0.wrapping_add(1), k1]);
            Self { keys: [k0, k1] }
        })
    }

    fn entropy() -> [u64; 2] {
        unsafe extern "C" {
            fn getentropy(buffer: *mut core::ffi::c_void, length: usize) -> i32;
        }
        let mut keys = [0u64; 2];
        // SAFETY: the buffer spans sixteen initialized writable bytes, below
        // getentropy's 256-byte limit. The OS fills every byte on success.
        if unsafe { getentropy(keys.as_mut_ptr() as *mut core::ffi::c_void, 16) } != 0 {
            core::panicking::abort_now();
        }
        keys
    }
}
impl Default for RandomState {
    fn default() -> Self { Self::new() }
}
impl BuildHasher for RandomState {
    type Hasher = DefaultHasher;
    fn build_hasher(&self) -> DefaultHasher { DefaultHasher::with_keys(self.keys[0], self.keys[1]) }
}
impl core::fmt::Debug for RandomState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.write_str("RandomState { .. }") }
}

/// SipHash-1-3. The algorithm is an implementation detail of this library.
#[derive(Clone)]
pub struct DefaultHasher {
    v0: u64, v1: u64, v2: u64, v3: u64,
    tail: u64, tail_len: u32, length: u64,
}
impl DefaultHasher {
    pub fn new() -> Self { Self::with_keys(0, 0) }
    fn with_keys(k0: u64, k1: u64) -> Self {
        Self {
            v0: k0 ^ 0x736f6d6570736575, v1: k1 ^ 0x646f72616e646f6d,
            v2: k0 ^ 0x6c7967656e657261, v3: k1 ^ 0x7465646279746573,
            tail: 0, tail_len: 0, length: 0,
        }
    }
    fn round(&mut self) {
        self.v0 = self.v0.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(13); self.v1 ^= self.v0; self.v0 = self.v0.rotate_left(32);
        self.v2 = self.v2.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(16); self.v3 ^= self.v2;
        self.v0 = self.v0.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(21); self.v3 ^= self.v0;
        self.v2 = self.v2.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(17); self.v1 ^= self.v2; self.v2 = self.v2.rotate_left(32);
    }
    fn compress(&mut self, word: u64) {
        self.v3 ^= word; self.round(); self.v0 ^= word;
    }
}
impl Default for DefaultHasher {
    fn default() -> Self { Self::new() }
}
impl Hasher for DefaultHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        for &byte in bytes {
            self.tail |= (byte as u64) << (self.tail_len * 8);
            self.tail_len += 1;
            if self.tail_len == 8 {
                self.compress(self.tail); self.tail = 0; self.tail_len = 0;
            }
        }
    }
    fn finish(&self) -> u64 {
        let mut state = self.clone();
        state.compress(self.tail | ((self.length & 0xff) << 56));
        state.v2 ^= 0xff;
        state.round(); state.round(); state.round();
        state.v0 ^ state.v1 ^ state.v2 ^ state.v3
    }
}
