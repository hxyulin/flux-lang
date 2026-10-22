#[cfg(target_arch = "aarch64")]
mod neon {
    use core::arch::aarch64::*;

    #[derive(Copy, Clone)]
    #[repr(C)]
    struct Finder {
        bytes: uint8x16_t,
        len: usize,
    }

    #[derive(Copy, Clone)]
    union Kind {
        finder: Finder,
        tiny: u8,
    }

    #[inline(never)]
    fn bounce(kind: Kind) -> Kind {
        kind
    }

    #[inline(never)]
    pub fn run(input: [u8; 16]) {
        unsafe {
            let kind = Kind {
                finder: Finder {
                    bytes: vld1q_u8(input.as_ptr()),
                    len: input.len(),
                },
            };
            let copy = bounce(kind.clone());
            let lanes = vreinterpretq_u64_u8(copy.finder.bytes);
            println!(
                "{} {} {} {} {}",
                core::mem::size_of::<Kind>(),
                core::mem::align_of::<Kind>(),
                copy.finder.len,
                vgetq_lane_u64(lanes, 0),
                vgetq_lane_u64(lanes, 1)
            );
            let narrow = bounce(Kind { tiny: input[0] });
            println!("{}", narrow.clone().tiny);
        }
    }
}

fn main() {
    #[cfg(target_arch = "aarch64")]
    {
        neon::run([
            0, 1, 2, 3, 4, 5, 6, 7, 128, 129, 130, 131, 252, 253, 254, 255,
        ]);
        neon::run([255; 16]);
    }
}
