use core::arch::aarch64::*;
macro_rules! probe {
    ($lane:ty,$count:expr,$load:ident,$dup:ident,$gt:ident,$eq:ident,$and:ident,$add:ident,$sub:ident,$sum:ident) => {{
        let mut bytes = [0u8;17];
        let values: [$lane;$count] = core::array::from_fn(|i| if i == 0 { <$lane>::MAX } else { i as $lane });
        // SAFETY: both regions hold exactly sixteen readable/writable bytes;
        // the byte copy intentionally produces an unaligned vector source.
        unsafe { core::ptr::copy_nonoverlapping(values.as_ptr() as *const u8,bytes.as_mut_ptr().add(1),16); }
        // SAFETY: NEON is baseline on AArch64 and the unaligned source holds
        // sixteen initialized bytes. Vector transmutations preserve all bits.
        unsafe {
            let vector = $load(bytes.as_ptr().add(1) as *const $lane);
            let two = $dup(2);
            let masks: [$lane;$count] = core::mem::transmute($gt(vector,two));
            let equals: [$lane;$count] = core::mem::transmute($eq(vector,two));
            let sums: [$lane;$count] = core::mem::transmute($add(vector,two));
            let differences: [$lane;$count] = core::mem::transmute($sub(vector,two));
            let masked: [$lane;$count] = core::mem::transmute($and(vector,two));
            let mut total: $lane = 0;
            for i in 0..$count {
                assert_eq!(masks[i],if values[i]>2 {<$lane>::MAX}else{0});
                assert_eq!(equals[i],if values[i]==2 {<$lane>::MAX}else{0});
                assert_eq!(sums[i],values[i].wrapping_add(2));
                assert_eq!(differences[i],values[i].wrapping_sub(2));
                assert_eq!(masked[i],values[i]&2);
                total=total.wrapping_add(values[i]);
            }
            assert_eq!($sum(vector),total);
            println!("{} {} {:?}",core::mem::size_of::<$lane>(),total,differences);
        }
    }};
}
struct Dropped<'a>(&'a core::cell::Cell<u32>);
impl Drop for Dropped<'_> { fn drop(&mut self) {self.0.set(self.0.get()+1);} }

fn zip_bytes() {
    let left: [u8; 16] = core::array::from_fn(|i| i as u8);
    let right: [u8; 16] = core::array::from_fn(|i| 32 + i as u8);
    // SAFETY: NEON is baseline on AArch64, both arrays hold 16 bytes, and
    // transmuting the vectors to byte arrays preserves all bits.
    let (low, high): ([u8; 16], [u8; 16]) = unsafe {
        let a = vld1q_u8(left.as_ptr());
        let b = vld1q_u8(right.as_ptr());
        (
            core::mem::transmute(vzip1q_u8(a, b)),
            core::mem::transmute(vzip2q_u8(a, b)),
        )
    };
    for i in 0..8 {
        assert_eq!([low[2 * i], low[2 * i + 1]], [left[i], right[i]]);
        assert_eq!([high[2 * i], high[2 * i + 1]], [left[i + 8], right[i + 8]]);
    }
    println!("{low:?} {high:?}");
}

fn main() {
    zip_bytes();
    probe!(u16,8,vld1q_u16,vdupq_n_u16,vcgtq_u16,vceqq_u16,vandq_u16,vaddq_u16,vsubq_u16,vaddvq_u16);
    probe!(u32,4,vld1q_u32,vdupq_n_u32,vcgtq_u32,vceqq_u32,vandq_u32,vaddq_u32,vsubq_u32,vaddvq_u32);
    probe!(u64,2,vld1q_u64,vdupq_n_u64,vcgtq_u64,vceqq_u64,vandq_u64,vaddq_u64,vsubq_u64,vaddvq_u64);
    let dropped=core::cell::Cell::new(0);
    let selected=core::hint::select_unpredictable(true,Dropped(&dropped),Dropped(&dropped));
    assert_eq!(dropped.get(),1);
    drop(selected);
    assert_eq!(dropped.get(),2);
    assert_eq!(core::hint::select_unpredictable(false,3,5),5);
}
