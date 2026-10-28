use core::mem::{align_of, size_of};

#[derive(Copy, Clone)]
#[repr(C)]
struct Record {
    data: *mut u8,
    len: usize,
    next: *const Record,
}

#[derive(Copy, Clone)]
#[repr(C)]
union Payload {
    record: Record,
    pointer: *mut Record,
    words: [usize; 3],
}

#[derive(Copy, Clone)]
union Number {
    real: f64,
    bits: u64,
}

#[derive(Copy, Clone)]
union Character {
    character: char,
    bits: u32,
}

#[derive(Copy, Clone)]
union Pair {
    pair: (u32, bool),
    words: [u8; 8],
}

#[repr(packed)]
union Packed {
    pointer: *const u8,
    words: [u8; 8],
}

fn bounce(payload: Payload) -> Payload {
    payload
}

fn main() {
    assert_eq!(size_of::<Payload>(), 3 * size_of::<usize>());
    assert_eq!(align_of::<Payload>(), align_of::<usize>());
    let mut bytes = [7u8, 11, 19];
    let mut record = Record {
        data: bytes.as_mut_ptr(),
        len: bytes.len(),
        next: core::ptr::null(),
    };
    let mut payload = bounce(Payload { record });
    let cloned = payload.clone();
    unsafe {
        let loaded = cloned.record;
        assert_eq!(loaded.len, 3);
        assert!(loaded.next.is_null());
        assert_eq!(loaded.data, bytes.as_mut_ptr());
        *loaded.data.add(1) = 42;
        payload.pointer = &mut record;
        (*payload.pointer).len = 2;
        assert_eq!((*payload.pointer).data, bytes.as_mut_ptr());
    }
    assert_eq!(record.len, 2);
    assert_eq!(bytes[1], 42);
    let number = Number {
        bits: 1.5f64.to_bits(),
    };
    let character = Character { bits: '界' as u32 };
    let pair = Pair { pair: (42, true) };
    let packed = Packed {
        pointer: bytes.as_ptr(),
    };
    unsafe {
        assert_eq!(number.real, 1.5);
        assert_eq!(character.character, '界');
        assert_eq!(pair.clone().pair, (42, true));
        let pointer = packed.pointer;
        assert_eq!(pointer, bytes.as_ptr());
        assert_eq!(*pointer, 7);
        println!(
            "union fields {} {} {} {}",
            number.real, character.character, pair.pair.0, bytes[1]
        );
    }
}
