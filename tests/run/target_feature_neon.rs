use core::arch::aarch64::*;

#[target_feature(enable = "neon")]
unsafe fn count(bytes: *const u8, needle: u8) -> u64 {
    unsafe {
        let mask = vceqq_u8(vld1q_u8(bytes), vdupq_n_u8(needle));
        vgetq_lane_u64::<0>(vreinterpretq_u64_u8(mask))
    }
}

struct Search(u8);
impl Search {
    #[cfg_attr(target_feature = "neon", target_feature(enable = "neon"))]
    #[inline]
    unsafe fn find(&self, bytes: *const u8) -> u64 {
        unsafe { count(bytes, self.0) }
    }
}

trait Count {
    #[target_feature(enable = "neon")]
    unsafe fn value(&self) -> u8 { 3 }
}
impl Count for Search {
    #[target_feature(enable = "neon")]
    unsafe fn value(&self) -> u8 { self.0 }
}

#[target_feature(enable = "neon,neon", enable = "neon")]
#[target_feature(enable = "neon")]
const unsafe fn number() -> u8 { 3 }

fn main() {
    let bytes = [1u8, 3, 3, 4, 5, 3, 7, 8, 9, 3, 11, 12, 3, 14, 15, 3];
    let f: unsafe fn(*const u8, u8) -> u64 = count;
    let search = Search(unsafe { number() });
    unsafe {
        println!("{} {} {}", f(bytes.as_ptr(), 3), search.find(bytes.as_ptr()), search.value());
    }
}
