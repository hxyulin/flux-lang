//! Raw pointers from references, by coercion and by `as`, casts
//! between pointer types and to and from integers, `as_ptr` on slices
//! and arrays, and printing byte strings through `printf`'s `%.*s`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn read(p: *const i32) -> i32 {
    unsafe { *p }
}

fn write(p: *mut i32, value: i32) {
    unsafe { *p = value };
}

fn print(bytes: &[u8]) {
    unsafe { printf(c"[%.*s]\n".as_ptr(), bytes.len() as i32, bytes.as_ptr()) };
}

struct Pair {
    a: i32,
    b: i64,
}

fn main() {
    let mut x = 5;
    let r = &x;
    let p: *const i32 = r;
    let q = r as *const i32;
    unsafe { printf(c"%d %d %d\n".as_ptr(), read(p), read(q), read(&x)) };
    write(&mut x, 7);
    let m = &mut x;
    write(m, *m + 1);
    let raw = m as *mut i32;
    unsafe { *raw += 1 };
    *m += 1;
    let as_const: *const i32 = m;
    unsafe { printf(c"%d %d\n".as_ptr(), x, read(as_const)) };

    let bytes = *b"hello, world";
    let slice: &[u8] = &bytes;
    print(slice);
    print(b"byte string");
    let first = slice.as_ptr();
    let whole = slice as *const [u8];
    let again = whole as *const u8;
    let third = &bytes as *const u8;
    let address = first as usize;
    let back = address as *const u8;
    unsafe {
        printf(
            c"%c %c %c %c %d %d\n".as_ptr(),
            *first as i32,
            *again as i32,
            *third as i32,
            *back as i32,
            (address == again as usize) as i32,
            (first as u64 % 1) as i32,
        );
    }
    let mut buffer = [b'.'; 8];
    let whole_buffer: &mut [u8] = &mut buffer;
    let out = whole_buffer.as_mut_ptr();
    unsafe {
        *out = b'a';
        *(out as usize as *mut u8) = b'b';
    }
    let view: &mut [u8] = &mut buffer;
    view[2] = b'c';
    let w = view.as_mut_ptr();
    unsafe { *w = b'z' };
    print(&buffer);

    let pair = Pair { a: 3, b: 4 };
    let pp = &pair as *const Pair;
    let pa = &pair.a as *const i32;
    let as_byte = pp as *const u8;
    unsafe {
        printf(
            c"%d %lld %d %d\n".as_ptr(),
            (*pp).a,
            (*pp).b,
            *pa,
            (as_byte as usize == pp as usize) as i32,
        );
    }
    let signed = -1i32 as *const u8;
    let wide = signed as usize;
    let narrow = signed as u8;
    unsafe { printf(c"%d %d\n".as_ptr(), (wide == usize::MAX) as i32, narrow as i32) };
}
