//! Hashing: `Hash` for values that can be hashed, `Hasher` for what
//! hashes them, and `BuildHasher` for what makes `Hasher`s. Each value
//! writes to its hasher exactly what rustc's does, in the same calls,
//! so a program's own hasher gives the same hashes as with rustc: an
//! integer through its `write_*` method, a `str` its bytes and then
//! `0xff`, a slice (and an array) its length through
//! `write_length_prefix` and then its elements, all at once for the
//! integers. `#[derive(Hash)]` hashes an enum's discriminant (as its
//! `repr` type, `isize` by default) if it has more than one variant,
//! then the fields in order.

use crate::default::Default;
use crate::intrinsics::slice_from_raw_parts;
use crate::marker::PhantomData;

/// A value that can be hashed.
#[lang = "Hash"]
pub trait Hash {
    /// Feeds the value to `state`.
    fn hash<H: Hasher>(&self, state: &mut H);

    /// Feeds each value of `data` to `state`, in order.
    fn hash_slice<H: Hasher>(data: &[Self], state: &mut H)
    where
        Self: Sized,
    {
        let mut i = 0;
        while i < data.len() {
            data[i].hash(state);
            i += 1;
        }
    }
}

/// What hashes values: fed bytes and integers, it gives a `u64`.
pub trait Hasher {
    /// The hash of what it was fed.
    fn finish(&self) -> u64;

    /// Feeds it bytes.
    fn write(&mut self, bytes: &[u8]);

    /// Feeds it a `u8`.
    fn write_u8(&mut self, i: u8) {
        self.write(&[i])
    }

    /// Feeds it a `u16`.
    fn write_u16(&mut self, i: u16) {
        self.write(&i.to_ne_bytes())
    }

    /// Feeds it a `u32`.
    fn write_u32(&mut self, i: u32) {
        self.write(&i.to_ne_bytes())
    }

    /// Feeds it a `u64`.
    fn write_u64(&mut self, i: u64) {
        self.write(&i.to_ne_bytes())
    }

    /// Feeds it a `u128`.
    fn write_u128(&mut self, i: u128) {
        self.write(&i.to_ne_bytes())
    }

    /// Feeds it a `usize`.
    fn write_usize(&mut self, i: usize) {
        self.write(&i.to_ne_bytes())
    }

    /// Feeds it a `i8`.
    fn write_i8(&mut self, i: i8) {
        self.write_u8(i as u8)
    }

    /// Feeds it a `i16`.
    fn write_i16(&mut self, i: i16) {
        self.write_u16(i as u16)
    }

    /// Feeds it a `i32`.
    fn write_i32(&mut self, i: i32) {
        self.write_u32(i as u32)
    }

    /// Feeds it a `i64`.
    fn write_i64(&mut self, i: i64) {
        self.write_u64(i as u64)
    }

    /// Feeds it a `i128`.
    fn write_i128(&mut self, i: i128) {
        self.write_u128(i as u128)
    }

    /// Feeds it a `isize`.
    fn write_isize(&mut self, i: isize) {
        self.write_usize(i as usize)
    }

    /// Feeds it the length of a collection, before its elements.
    fn write_length_prefix(&mut self, len: usize) {
        self.write_usize(len);
    }

    /// Feeds it a string: its bytes, then `0xff`, which no UTF-8 has.
    fn write_str(&mut self, s: &str) {
        self.write(s.as_bytes());
        self.write_u8(0xff);
    }
}

/// A mutable reference to a hasher is one.
impl<H: Hasher> Hasher for &mut H {
    fn finish(&self) -> u64 {
        (**self).finish()
    }
    fn write(&mut self, bytes: &[u8]) {
        (**self).write(bytes)
    }

    fn write_u8(&mut self, i: u8) {
        (**self).write_u8(i)
    }

    fn write_u16(&mut self, i: u16) {
        (**self).write_u16(i)
    }

    fn write_u32(&mut self, i: u32) {
        (**self).write_u32(i)
    }

    fn write_u64(&mut self, i: u64) {
        (**self).write_u64(i)
    }

    fn write_u128(&mut self, i: u128) {
        (**self).write_u128(i)
    }

    fn write_usize(&mut self, i: usize) {
        (**self).write_usize(i)
    }

    fn write_i8(&mut self, i: i8) {
        (**self).write_i8(i)
    }

    fn write_i16(&mut self, i: i16) {
        (**self).write_i16(i)
    }

    fn write_i32(&mut self, i: i32) {
        (**self).write_i32(i)
    }

    fn write_i64(&mut self, i: i64) {
        (**self).write_i64(i)
    }

    fn write_i128(&mut self, i: i128) {
        (**self).write_i128(i)
    }

    fn write_isize(&mut self, i: isize) {
        (**self).write_isize(i)
    }

    fn write_length_prefix(&mut self, len: usize) {
        (**self).write_length_prefix(len)
    }
    fn write_str(&mut self, s: &str) {
        (**self).write_str(s)
    }
}

/// What makes `Hasher`s, each the same.
pub trait BuildHasher {
    /// The hashers it makes.
    type Hasher: Hasher;

    /// A new hasher.
    fn build_hasher(&self) -> Self::Hasher;

    /// The hash of `x` by a new hasher.
    fn hash_one<T: Hash>(&self, x: T) -> u64
    where
        Self: Sized,
    {
        let mut hasher = self.build_hasher();
        x.hash(&mut hasher);
        hasher.finish()
    }
}

/// A `BuildHasher` that makes each hasher with `H::default()`.
pub struct BuildHasherDefault<H> {
    marker: PhantomData<H>,
}

impl<H> BuildHasherDefault<H> {
    /// One for `H`.
    pub const fn new() -> BuildHasherDefault<H> {
        BuildHasherDefault { marker: PhantomData }
    }
}

impl<H: Default + Hasher> BuildHasher for BuildHasherDefault<H> {
    type Hasher = H;

    fn build_hasher(&self) -> H {
        H::default()
    }
}

impl<H> Default for BuildHasherDefault<H> {
    fn default() -> BuildHasherDefault<H> {
        BuildHasherDefault { marker: PhantomData }
    }
}

impl<H> crate::clone::Clone for BuildHasherDefault<H> {
    fn clone(&self) -> BuildHasherDefault<H> {
        BuildHasherDefault { marker: PhantomData }
    }
}

impl<H> crate::cmp::PartialEq for BuildHasherDefault<H> {
    fn eq(&self, _other: &BuildHasherDefault<H>) -> bool {
        true
    }
}

impl<H> crate::cmp::Eq for BuildHasherDefault<H> {}

// The integers hash through their `write_*` methods, and slices of them
// as one `write` of their bytes.

impl Hash for u8 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u8(*self)
    }

    fn hash_slice<H: Hasher>(data: &[u8], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 1) };
        state.write(bytes)
    }
}

impl Hash for u16 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u16(*self)
    }

    fn hash_slice<H: Hasher>(data: &[u16], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 2) };
        state.write(bytes)
    }
}

impl Hash for u32 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u32(*self)
    }

    fn hash_slice<H: Hasher>(data: &[u32], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 4) };
        state.write(bytes)
    }
}

impl Hash for u64 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(*self)
    }

    fn hash_slice<H: Hasher>(data: &[u64], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 8) };
        state.write(bytes)
    }
}

impl Hash for u128 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u128(*self)
    }

    fn hash_slice<H: Hasher>(data: &[u128], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 16) };
        state.write(bytes)
    }
}

impl Hash for usize {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(*self)
    }

    fn hash_slice<H: Hasher>(data: &[usize], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 8) };
        state.write(bytes)
    }
}

impl Hash for i8 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i8(*self)
    }

    fn hash_slice<H: Hasher>(data: &[i8], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 1) };
        state.write(bytes)
    }
}

impl Hash for i16 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i16(*self)
    }

    fn hash_slice<H: Hasher>(data: &[i16], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 2) };
        state.write(bytes)
    }
}

impl Hash for i32 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i32(*self)
    }

    fn hash_slice<H: Hasher>(data: &[i32], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 4) };
        state.write(bytes)
    }
}

impl Hash for i64 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i64(*self)
    }

    fn hash_slice<H: Hasher>(data: &[i64], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 8) };
        state.write(bytes)
    }
}

impl Hash for i128 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i128(*self)
    }

    fn hash_slice<H: Hasher>(data: &[i128], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 16) };
        state.write(bytes)
    }
}

impl Hash for isize {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_isize(*self)
    }

    fn hash_slice<H: Hasher>(data: &[isize], state: &mut H) {
        let bytes = unsafe { slice_from_raw_parts(data.as_ptr() as *const u8, data.len() * 8) };
        state.write(bytes)
    }
}

impl Hash for bool {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u8(*self as u8)
    }
}

impl Hash for char {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u32(*self as u32)
    }
}

impl Hash for str {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_str(self);
    }
}

impl Hash for () {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}

impl<A: Hash> Hash for (A,) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
    }
}

impl<A: Hash, B: Hash> Hash for (A, B) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
    }
}

impl<A: Hash, B: Hash, C: Hash> Hash for (A, B, C) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
    }
}

impl<A: Hash, B: Hash, C: Hash, D: Hash> Hash for (A, B, C, D) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
        self.3.hash(state);
    }
}

impl<A: Hash, B: Hash, C: Hash, D: Hash, E: Hash> Hash for (A, B, C, D, E) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
        self.3.hash(state);
        self.4.hash(state);
    }
}

impl<A: Hash, B: Hash, C: Hash, D: Hash, E: Hash, F: Hash> Hash for (A, B, C, D, E, F) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
        self.3.hash(state);
        self.4.hash(state);
        self.5.hash(state);
    }
}

impl<A: Hash, B: Hash, C: Hash, D: Hash, E: Hash, F: Hash, G: Hash> Hash for (A, B, C, D, E, F, G) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
        self.3.hash(state);
        self.4.hash(state);
        self.5.hash(state);
        self.6.hash(state);
    }
}

impl<A: Hash, B: Hash, C: Hash, D: Hash, E: Hash, F: Hash, G: Hash, H: Hash> Hash for (A, B, C, D, E, F, G, H) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
        self.3.hash(state);
        self.4.hash(state);
        self.5.hash(state);
        self.6.hash(state);
        self.7.hash(state);
    }
}

impl<A: Hash, B: Hash, C: Hash, D: Hash, E: Hash, F: Hash, G: Hash, H: Hash, I: Hash> Hash for (A, B, C, D, E, F, G, H, I) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
        self.3.hash(state);
        self.4.hash(state);
        self.5.hash(state);
        self.6.hash(state);
        self.7.hash(state);
        self.8.hash(state);
    }
}

impl<A: Hash, B: Hash, C: Hash, D: Hash, E: Hash, F: Hash, G: Hash, H: Hash, I: Hash, J: Hash> Hash for (A, B, C, D, E, F, G, H, I, J) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
        self.3.hash(state);
        self.4.hash(state);
        self.5.hash(state);
        self.6.hash(state);
        self.7.hash(state);
        self.8.hash(state);
        self.9.hash(state);
    }
}

impl<A: Hash, B: Hash, C: Hash, D: Hash, E: Hash, F: Hash, G: Hash, H: Hash, I: Hash, J: Hash, K: Hash> Hash for (A, B, C, D, E, F, G, H, I, J, K) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
        self.3.hash(state);
        self.4.hash(state);
        self.5.hash(state);
        self.6.hash(state);
        self.7.hash(state);
        self.8.hash(state);
        self.9.hash(state);
        self.10.hash(state);
    }
}

impl<A: Hash, B: Hash, C: Hash, D: Hash, E: Hash, F: Hash, G: Hash, H: Hash, I: Hash, J: Hash, K: Hash, L: Hash> Hash for (A, B, C, D, E, F, G, H, I, J, K, L) {
    fn hash<__H: Hasher>(&self, state: &mut __H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
        self.3.hash(state);
        self.4.hash(state);
        self.5.hash(state);
        self.6.hash(state);
        self.7.hash(state);
        self.8.hash(state);
        self.9.hash(state);
        self.10.hash(state);
        self.11.hash(state);
    }
}

impl<T: Hash> Hash for [T] {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_length_prefix(self.len());
        Hash::hash_slice(self, state)
    }
}

impl<T: Hash, const N: usize> Hash for [T; N] {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&self[..], state)
    }
}

impl<T: Hash> Hash for &T {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<T: Hash> Hash for &mut T {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

/// A pointer hashes its address.
impl<T> Hash for *const T {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(*self as usize);
    }
}

impl<T> Hash for *mut T {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(*self as usize);
    }
}
