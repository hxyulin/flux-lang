#[repr(transparent)]
struct Wrap<T: ?Sized>(T);
#[repr(transparent)]
struct Text(str);
#[repr(transparent)]
struct Outer(Wrap<[u8]>);
#[repr(C)]
struct Header<T: ?Sized> { tag: u8, tail: T }
#[repr(C)]
struct Aligned<T: ?Sized> { tag: u64, tail: T }
#[repr(C)]
struct Inferred<P, T: ?Sized> { prefix: P, tail: T }
const VALUES: Wrap<[u16; 3]> = Wrap([43, 47, 53]);
static BORROWED: &Wrap<[u16]> = &VALUES;
struct Count<'a>(&'a std::cell::Cell<u32>);
impl Drop for Count<'_> {
    fn drop(&mut self) { self.0.set(self.0.get() + 1); }
}
fn wrap<T: ?Sized>(value: &T) -> &Wrap<T> {
    unsafe { &*(value as *const T as *const Wrap<T>) }
}
fn text(value: &str) -> &Text { unsafe { &*(value as *const str as *const Text) } }
fn outer(value: &[u8]) -> &Outer { unsafe { &*(value as *const [u8] as *const Outer) } }
fn inner<T: ?Sized>(value: &Wrap<T>) -> &T { &value.0 }
fn mutable(value: &mut [u8]) -> &mut Wrap<[u8]> { unsafe { &mut *(value as *mut [u8] as *mut Wrap<[u8]>) } }
fn lengths(value: &Wrap<[u16]>) -> usize { value.0.len() }
fn main() {
    let t = text("compiler");
    let o = outer(b"metadata");
    assert_eq!(&t.0, "compiler");
    assert_eq!(&o.0.0, b"metadata");
    assert_eq!(std::mem::size_of::<&Text>(), 16);
    assert_eq!(std::mem::size_of::<*const Outer>(), 16);
    assert_eq!(BORROWED.0.len(), 3);
    assert_eq!(BORROWED.0[2], 53);
    assert_eq!(std::mem::size_of_val(BORROWED), 6);
    assert_eq!(std::mem::size_of_val(t), 8);
    assert_eq!(std::mem::align_of_val(t), 1);
    let source = Wrap([11u16, 13, 17]);
    let inferred = Inferred { prefix: 5u32, tail: [1u8, 2] };
    let inferred_ref: &Inferred<_, [u8]> = &inferred;
    assert_eq!(inferred_ref.prefix, 5u32);
    assert_eq!(inferred_ref.tail[1], 2);
    let inferred_box: Box<Inferred<_, [u8]>> = Box::new(Inferred { prefix: 9u32, tail: [3u8, 4] });
    assert_eq!(inferred_box.prefix, 9u32);
    assert_eq!(inferred_box.tail[1], 4);
    let raw: *const Inferred<_, [u8]> = &inferred as *const Inferred<u32, [u8; 2]>;
    let from_ref: *const Inferred<_, [u8]> = &inferred;
    let cast = (&inferred as *const Inferred<u32, [u8; 2]>) as *const Inferred<u32, [u8]>;
    unsafe { assert_eq!((*cast).tail[0], 1); }
    unsafe { assert_eq!((*raw).prefix, 5u32); assert_eq!((*from_ref).tail[1], 2); }
    let mut inferred_mut = Inferred { prefix: 11u32, tail: [7u8, 8] };
    let raw_mut: *mut Inferred<_, [u8]> = &mut inferred_mut as *mut Inferred<u32, [u8; 2]>;
    unsafe { (*raw_mut).tail[1] = 17; }
    let raw_const: *const Inferred<_, [u8]> = &mut inferred_mut as *mut Inferred<u32, [u8; 2]>;
    unsafe { assert_eq!((*raw_const).tail[1], 17); }
    let coerced: &Wrap<[u16]> = &source;
    assert_eq!(coerced.0[2], 17);
    let boxed: Box<Wrap<[u16]>> = Box::new(Wrap([19u16, 23, 29]));
    assert_eq!(boxed.0[1], 23);
    let drops = std::cell::Cell::new(0);
    {
        let boxed: Box<Header<[Count<'_>]>> = Box::new(Header {
            tag: 7,
            tail: [Count(&drops), Count(&drops)],
        });
        assert_eq!(boxed.tag, 7);
        assert_eq!(boxed.tail.len(), 2);
    }
    assert_eq!(drops.get(), 2);
    let nested = wrap(wrap("nested"));
    assert_eq!(&inner(nested).0, "nested");
    assert_eq!(std::mem::size_of_val(nested), 6);
    let words: &[u16] = &[3,5,7];
    let w = wrap(words);
    assert_eq!(lengths(w), 3);
    assert_eq!(std::mem::size_of_val(w), 6);
    assert_eq!(std::mem::align_of_val(w), 2);
    let mut data = [1u8,2,3,4];
    { let m = mutable(&mut data); m.0[2] = 91; }
    assert_eq!(data[2], 91);
    let storage = Header { tag: 17, tail: [11u16,13,19] };
    let pointer = std::ptr::slice_from_raw_parts(&storage as *const Header<[u16;3]> as *const u16, 3);
    let raw_h = unsafe { &*(pointer as *const Header<[u16]>) };
    let h: &Header<[u16]> = &storage;
    assert_eq!(raw_h.tail[2], h.tail[2]);
    assert_eq!(h.tag, 17); assert_eq!(h.tail[2], 19);
    assert_eq!(std::mem::size_of_val(h), 8);
    assert_eq!(std::mem::align_of_val(h), 2);
    let storage = Aligned { tag: 123, tail: [31u8,37,41] };
    let pointer = std::ptr::slice_from_raw_parts(&storage as *const Aligned<[u8;3]> as *const u8, 3);
    let a = unsafe { &*(pointer as *const Aligned<[u8]>) };
    assert_eq!(a.tag, 123); assert_eq!(a.tail[1], 37);
    assert_eq!(std::mem::size_of_val(a), 16);
    assert_eq!(std::mem::align_of_val(a), 8);
    assert_eq!(t as *const Text as *const u8, "compiler".as_ptr());
    println!("{} {} {} {} {} {}", &t.0, o.0.0.len(), lengths(w), data[2], h.tail[2], a.tail[1]);
}
