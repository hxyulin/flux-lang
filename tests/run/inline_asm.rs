use core::arch::asm;

fn index(count: &mut usize) -> usize {
    let i = *count;
    *count += 1;
    i
}

fn main() {
    let source=19u64;
    let mut total=4u64;
    let mut float=2.0f64;
    let source_float=3.5f64;
    unsafe {
        asm!("add {total}, {total}, {source}", "fadd {float:d}, {float:d}, {source_float:d}",
            source=in(reg)source, total=inout(reg)total,
            source_float=in(vreg)source_float, float=inout(vreg)float,
            options(nomem,nostack,preserves_flags));
        asm!("prfm pldl1keep, [{line}]",line=in(reg)&source as *const u64,options(readonly,nostack,preserves_flags));
    }
    assert_eq!(total,23);
    assert_eq!(float,5.5);
    let mut stored=0u64;
    unsafe {
        asm!("str {value}, [{pointer}]",pointer=in(reg)&raw mut stored,value=in(reg)(source+7));
    }
    assert_eq!(stored,26);
    let mut input_calls=0usize;
    let read=|| unsafe {asm!("/*{0}*/",in(reg)source)};
    read();read();
    unsafe {asm!("/*{0}*/",in(reg)index(&mut input_calls));}
    assert_eq!(input_calls,1);
    let mut x = 11u64;
    let mut y = 7u64;
    unsafe {
        asm!("add {x}, {x}, {y}", "eor {y}, {y}, {x}", x=inout(reg)x, y=inout(reg)y);
    }
    assert_eq!((x, y), (18, 21));
    let mut w = 0xff00u32;
    unsafe {
        core::arch::asm!("lsr {0:w}, {0:w}, #8",inout(reg)w,options(nomem,nostack,preserves_flags));
    }
    assert_eq!(w, 255);
    let mut data = [13u64, 17];
    let mut count = 0usize;
    unsafe {
        asm!("add {0:x}, {0:x}, #3", "add {1:x}, {1:x}, #5",inout(reg)data[index(&mut count)],inout(reg)data[index(&mut count)]);
    }
    assert_eq!(count, 2);
    assert_eq!(data, [16, 22]);
    let mut value = 2u64;
    let before = value;
    let mut pointer = &raw mut value;
    let mut new = 41u64;
    unsafe {
        asm!("str {new:x}, [{pointer:x}]",new=inout(reg)new,pointer=inout(reg)pointer);
    }
    assert_eq!(before, 2);
    assert_eq!(value, 41);
    let mut f = 1.25f32;
    let mut d = -2.5f64;
    unsafe {
        asm!("/*{0:x} {1:x}*/",inout(reg)f,inout(reg)d,options(nomem,preserves_flags));
    }
    assert_eq!(f.to_bits(), 1.25f32.to_bits());
    assert_eq!(d.to_bits(), (-2.5f64).to_bits());
    let mut small = 173u8;
    let mut signed = -1024i64;
    unsafe {
        asm!("/*{0:b} {1:d}*/",inout(vreg)small,inout(vreg)signed);
    }
    assert_eq!(small, 173);
    assert_eq!(signed, -1024);
    let mut bits = 0xd971a35c6bd129efu64;
    for _ in 0..256 {
        bits = bits.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut actual = bits;
        unsafe {
            asm!("add {0:x}, {0:x}, #37", inout(reg) actual, options(nomem));
        }
        assert_eq!(actual, bits.wrapping_add(37));
    }
    let mut flip = 1.25f32;
    unsafe {
        asm!("eor {0:w}, {0:w}, #0x80000000", inout(reg) flip, options(nomem));
    }
    assert_eq!(flip.to_bits(), (-1.25f32).to_bits());
    let mut integer = -9i64;
    unsafe {
        asm!("fmov {0:d}, #1.0", inout(vreg) integer, options(nomem));
    }
    assert_eq!(integer, 1.0f64.to_bits() as i64);
    let mut byte = 255u8;
    unsafe {
        asm!("fmov {0:s}, #1.0", inout(vreg) byte, options(nomem));
    }
    assert_eq!(byte, 0);
    let mut literal = 1u64;
    unsafe {
        asm!("/*100% {{}} {0}*/", inout(reg) literal);
    }
    assert_eq!(literal, 1);
    let mut count = 4u64;
    {
        let mut next = || unsafe {
            asm!("add {0:x}, {0:x}, #1",inout(reg)count);
        };
        next();
        next();
    }
    assert_eq!(count, 6);
    macro_rules! barrier { ($place:expr) => { unsafe { asm!("/*{0:x}*/",inout(reg)$place); } }; }
    barrier!(count);
    unsafe {
        asm!("nop");
    }
    println!("asm {} {} {} {} {} {}", x, y, w, data[0], value, count);
}
