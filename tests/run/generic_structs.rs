//! Generic structs and impls: `Pair<T>` instantiated at u8, i16, i32,
//! u64, i128, bool, unit, tuples, structs with mixed field sizes and
//! nested `Pair<Pair<T>>`; tuple structs `Wrap<T>(T)`; structs with
//! several type parameters and with a generic field between small ones;
//! constructors through `Self`, consuming `self` methods returning T,
//! `&self` and `&mut self` methods, methods with their own type
//! parameters, impls for particular instances (`impl Pair<i32>`,
//! `impl Pair<u8>`, `impl<T> Pair<Pair<T>>`, `impl<T> Pair<(T, u8)>`),
//! turbofish on types and methods, `_` in body types, struct update
//! syntax, UFCS, constants of generic struct types, generic structs
//! passed by value and returned (sret), mutation through `&mut Pair<T>`,
//! generic structs in modules, and flux core's integer methods on fields
//! of instances.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn exit(code: i32) -> !;
}

struct Pair<T> {
    a: T,
    b: T,
}

impl<T> Pair<T> {
    fn new(a: T, b: T) -> Self {
        Self { a, b }
    }

    fn swap(self) -> Self {
        let Pair { a, b } = self;
        Pair { a: b, b: a }
    }

    fn first(self) -> T {
        self.a
    }

    fn second(self) -> T {
        self.b
    }

    fn set_a(&mut self, a: T) {
        self.a = a;
    }

    fn set_both(&mut self, a: T, b: T) {
        self.set_a(a);
        self.b = b;
    }

    fn with<U>(self, u: U) -> (T, U) {
        (self.a, u)
    }

    fn zip<U>(self, other: Pair<U>) -> Pair<(T, U)> {
        Pair {
            a: (self.a, other.a),
            b: (self.b, other.b),
        }
    }

    fn nest(self, other: Self) -> Pair<Self> {
        Pair::new(self, other)
    }

    fn tag(&self) -> u32 {
        7
    }

    fn rotate(self, n: u32) -> Self {
        if n == 0 {
            self
        } else {
            self.swap().rotate(n - 1)
        }
    }
}

impl Pair<i32> {
    fn sum(&self) -> i32 {
        self.a.wrapping_add(self.b)
    }

    fn describe(&self) -> u32 {
        32
    }
}

// The same names as `impl Pair<i32>`'s: the receiver's type picks one.
impl Pair<u8> {
    fn sum(&self) -> i32 {
        self.a as i32 * 1000 + self.b as i32
    }

    fn describe(&self) -> u32 {
        8
    }
}

impl Pair<Pair<u16>> {
    fn corners(&self) -> u64 {
        (self.a.a as u64) << 48
            | (self.a.b as u64) << 32
            | (self.b.a as u64) << 16
            | self.b.b as u64
    }
}

struct Wrap<T>(T);

impl<T> Wrap<T> {
    fn get(self) -> T {
        self.0
    }

    fn map_pair(self, other: T) -> Pair<T> {
        Pair::new(self.0, other)
    }

    fn put(&mut self, v: T) {
        self.0 = v;
    }
}

struct Both<A, B> {
    left: A,
    mid: u8,
    right: B,
}

impl<A, B> Both<A, B> {
    fn flip(self) -> Both<B, A> {
        Both {
            left: self.right,
            mid: self.mid + 1,
            right: self.left,
        }
    }

    fn into_tuple(self) -> (A, u8, B) {
        (self.left, self.mid, self.right)
    }

    fn bump(&mut self) {
        self.mid += 10;
    }
}

// `kind` is shared with `impl<T> Pair<(T, u8)>`, here at the same `T`.
impl<T> Pair<Pair<T>> {
    fn kind(&self) -> u32 {
        1
    }

    fn flatten(self) -> (T, T, T, T) {
        let Pair { a, b } = self;
        (a.a, a.b, b.a, b.b)
    }
}

impl<T> Pair<(T, u8)> {
    fn kind(&self) -> u32 {
        2
    }

    fn bytes(&self) -> u32 {
        self.a.1 as u32 + self.b.1 as u32
    }
}

// A generic field between small fields.
struct Sand<T> {
    x: u8,
    t: T,
    y: u16,
}

impl<T> Sand<T> {
    fn new(t: T) -> Self {
        Sand {
            x: 0xa5,
            t,
            y: 0xbeef,
        }
    }

    fn with_t<U>(self, t: U) -> Sand<U> {
        let rest = self_parts(self);
        Sand {
            x: rest.x,
            t,
            y: rest.y,
        }
    }

    fn bump(&mut self) {
        self.x = self.x.wrapping_add(0x60);
        self.y = self.y.rotate_left(4);
    }

    fn show(&self) {
        unsafe {
            printf(c"sand %x %x\n".as_ptr(), self.x as u32, self.y as u32);
        }
    }
}

fn self_parts<T>(s: Sand<T>) -> Sand<()> {
    Sand {
        x: s.x,
        t: (),
        y: s.y,
    }
}

struct Holder<T> {
    inner: Pair<T>,
    n: u32,
}

impl<T> Holder<T> {
    fn total(&self) -> u32 {
        self.inner.tag() + self.n
    }

    fn bump(&mut self) {
        self.n += self.inner.tag();
    }
}

const fn cid<T>(x: T) -> T {
    x
}

const P8: Pair<u8> = Pair { a: 1, b: 2 };
const P128: Pair<i128> = cid(Pair { a: -5, b: 1 << 100 });
const NESTED: Pair<Pair<bool>> = Pair {
    a: Pair { a: true, b: false },
    b: cid::<Pair<bool>>(Pair { a: false, b: true }),
};

struct Mixed {
    a: u8,
    b: u64,
    c: i16,
}

fn mixed(seed: u8) -> Mixed {
    Mixed {
        a: seed,
        b: seed as u64 * 0x1_0000_0001,
        c: -(seed as i16),
    }
}

fn show_mixed(m: &Mixed) {
    unsafe {
        printf(c"mixed %u %llx %d\n".as_ptr(), m.a as u32, m.b, m.c as i32);
    }
}

// Generic structs passed by value and returned.
fn make_pair<T>(a: T, b: T) -> Pair<T> {
    Pair { a, b }
}

fn swapped<T>(p: Pair<T>) -> Pair<T> {
    p.swap()
}

fn double_swap<T>(p: Pair<T>) -> Pair<T> {
    swapped(swapped(swapped(p)))
}

fn reset_i128(p: &mut Pair<i128>, to: i128) {
    p.a = to;
    p.b = -to;
}

fn reset<T>(p: &mut Pair<T>, a: T, b: T) {
    p.set_both(b, a);
}

fn show_i128(p: Pair<i128>) -> Pair<i128> {
    unsafe {
        printf(
            c"i128 pair %llx%016llx %llx%016llx\n".as_ptr(),
            (p.a >> 64) as u64,
            p.a as u64,
            (p.b >> 64) as u64,
            p.b as u64,
        );
    }
    p
}

mod shapes {
    pub struct Boxed<T> {
        pub inner: T,
        pub id: u16,
    }

    impl<T> Boxed<T> {
        pub fn new(inner: T, id: u16) -> Self {
            Boxed { inner, id }
        }

        pub fn open(self) -> T {
            self.inner
        }

        pub fn relabel(&mut self, id: u16) {
            self.id = id;
        }
    }

    pub fn boxed_pair<T>(a: T, b: T) -> Boxed<super::Pair<T>> {
        Boxed::new(super::Pair::new(a, b), 77)
    }
}

fn main() {
    // Many layouts.
    let p8 = Pair::new(1u8, 2u8);
    let p16 = Pair::<i16>::new(-300, 400);
    let p32: Pair<i32> = Pair {
        a: 2147483647,
        b: 1,
    };
    let p64 = make_pair(0xffff_0000_ffff_0000u64, 42);
    let pb = Pair::new(true, false).swap();
    let pu = Pair::new((), ());
    let pt: Pair<_> = Pair::new((1u8, 2u64), (3, 4));
    unsafe {
        printf(
            c"sums %d %d tags %u %u %u\n".as_ptr(),
            p8.sum(),
            p32.sum(),
            p8.describe(),
            p32.describe(),
            pu.tag() + p16.tag(),
        );
    }
    let ptt = pt.swap();
    unsafe {
        printf(
            c"layouts %d %d %llx %llu %d %d %u %llu\n".as_ptr(),
            p16.a as i32,
            p16.b as i32,
            p64.a,
            p64.b,
            pb.a as i32,
            pb.b as i32,
            ptt.a.0 as u32,
            ptt.b.1,
        );
    }
    let () = pu.first();

    // core methods on fields of instances.
    let neg = Pair::new(-5i32, i32::MIN);
    let pp = Pair::<u8>::new(250, 9);
    unsafe {
        printf(
            c"core %d %d %d %u %u %d\n".as_ptr(),
            neg.a.abs(),
            neg.b.wrapping_add(-1),
            i32::wrapping_mul(neg.a, 3),
            pp.a.wrapping_add(pp.b) as u32,
            p16.b.count_ones(),
            p16.a.signum() as i32,
        );
    }

    // Mutation through `&mut Pair<T>`.
    let mut m = Pair::new(10i128, 20i128);
    reset_i128(&mut m, 1 << 100);
    let m = show_i128(m);
    let mut m = double_swap(m);
    reset(&mut m, -1, 1 << 127);
    let mut m = show_i128(m);
    let mr = &mut m;
    mr.set_a(5);
    mr.b = mr.b.wrapping_add(1);
    show_i128(m);

    let mut ms = Pair::new(mixed(1), mixed(2));
    ms.set_a(mixed(3));
    reset(&mut ms, mixed(4), mixed(5));
    let ms = double_swap(ms);
    show_mixed(&ms.a);
    show_mixed(&ms.b);
    let msb = ms.second();
    show_mixed(&msb);

    // Nested generic structs.
    let nested = Pair::new(Pair::new(1u16, 2u16), Pair::new(3u16, 4u16));
    let c1 = nested.corners();
    let nested = Pair::new(nested.b.swap(), nested.a);
    let c2 = nested.corners();
    let deep = Pair::new(1u8, 2)
        .nest(Pair::new(3, 4))
        .nest(Pair::new(Pair::new(5, 6), Pair::new(7, 8)));
    let deep = deep.swap();
    let dd = deep.first().second().swap();
    unsafe {
        printf(c"nested %llx %llx %d\n".as_ptr(), c1, c2, dd.sum());
    }

    // Methods with their own type parameters.
    let w = Pair::new(-1i64, 2).with(true);
    let w2 = Pair::new(mixed(9), mixed(10)).with::<(u8, i128)>((5, -(1 << 80)));
    show_mixed(&w2.0);
    let z = Pair::new(1u8, 2).zip(Pair::new(-3i128, 4));
    let z2 = z.swap().zip(Pair::new((), ()));
    let ((zb8, zb128), ()) = z2.a;
    unsafe {
        printf(
            c"with %lld %d %u %lld zip %u %lld\n".as_ptr(),
            w.0,
            w.1 as i32,
            (w2.1).0 as u32,
            ((w2.1).1 >> 64) as i64,
            zb8 as u32,
            zb128 as i64,
        );
    }

    // Recursion on the same instance, by value.
    let mut i = 0;
    while i < 4 {
        let r = Pair::new(Wrap(i as u16), Wrap(100)).rotate(i);
        let rp = make_pair(mixed(i as u8), mixed(50)).rotate(i + 1);
        unsafe {
            printf(
                c"rotate %u %u %u\n".as_ptr(),
                i,
                r.a.0 as u32,
                r.second().get() as u32,
            );
        }
        show_mixed(&rp.a);
        i += 1;
    }

    // Tuple structs.
    let mut wr = Wrap(Pair::new(8u64, 9));
    wr.put(Pair::new(10, 11));
    wr.0.set_a(12);
    let inner = wr.get();
    let wp = Wrap(-2i16).map_pair(3);
    let wu = Wrap(()).map_pair(());
    let ww = Wrap(Wrap(Wrap(200u8))).get().get().get();
    unsafe {
        printf(
            c"wrap %llu %llu %d %d %u %u\n".as_ptr(),
            inner.a,
            inner.b,
            wp.a as i32,
            wp.b as i32,
            wu.tag(),
            ww as u32,
        );
    }

    // Several type parameters.
    let bo = Both {
        left: 1u8,
        mid: 2,
        right: -(1i128 << 64),
    };
    let mut fl = bo.flip();
    fl.bump();
    let back = fl.flip();
    let (l, mid, r) = back.into_tuple();
    let bm = Both::<Mixed, ()> {
        left: mixed(11),
        mid: 0,
        right: (),
    };
    let (bl, bmid, ()) = bm.flip().flip().into_tuple();
    show_mixed(&bl);
    unsafe {
        printf(
            c"both %u %u %lld %u\n".as_ptr(),
            l as u32,
            mid as u32,
            (r >> 64) as i64,
            bmid as u32,
        );
    }

    // Generic structs in modules.
    let mut bx = shapes::boxed_pair(-9i32, 9);
    bx.relabel(bx.id + 1);
    let id = bx.id;
    let bp = bx.open();
    let bb = shapes::Boxed::<bool>::new(true, 3);
    unsafe {
        printf(
            c"module %u %d %d %u\n".as_ptr(),
            id as u32,
            bp.sum(),
            bb.open() as i32,
            bp.describe(),
        );
    }

    // Generic fields between small fields, by value and through `&mut`.
    let mut s1 = Sand::new(-(1i128 << 120));
    s1.bump();
    s1.show();
    let s1_hi = (s1.t >> 64) as i64;
    let s2 = s1.with_t(Pair::new(7u64, 8));
    let mut s3 = s2.with_t(true);
    s3.bump();
    let s4 = Sand::new(()).with_t((1u8, 2i16));
    s3.show();
    s4.show();
    unsafe {
        printf(
            c"sand t %lld %d %u %d\n".as_ptr(),
            s1_hi,
            s3.t as i32,
            s4.t.0 as u32,
            s4.t.1 as i32,
        );
    }

    // Disjoint generic impls, nesting, consts and UFCS.
    let nn = Pair::new(Pair::new(1u8, 2), Pair::new(3, 4));
    let nt = Pair::new((1u8, 2u8), (3, 4));
    let k = nn.kind() * 10 + nt.kind() + nt.bytes();
    let (f0, _, _, f3) = nn.flatten();
    let mut h = Holder {
        inner: Pair::new(true, false),
        n: 1,
    };
    h.bump();
    let hr = &mut h;
    hr.bump();
    let ht = h.total();
    let ufcs = Pair::first(Pair::new(Wrap(5u8), Wrap(6))).0 as u32
        + Pair::<i16>::second(Pair::new(-7, 8)) as u32;
    let up = Pair {
        a: 100u64,
        ..Pair::new(1, 2)
    };
    let c128 = P128;
    unsafe {
        printf(
            c"misc %u %u %u %u %u %u %llu %llu\n".as_ptr(),
            k,
            f0 as u32 + f3 as u32,
            ht,
            ufcs,
            P8.swap().a as u32 + NESTED.b.b as u32 + NESTED.a.b as u32,
            P8.sum() as u32,
            up.a + up.b,
            (c128.b >> 64) as u64 + c128.a as u64,
        );
    }

    let code = Pair::new(30u8, 12).swap().first() as i32 + Pair::<i32>::new(-1, 1).sum();
    unsafe {
        exit(code);
    }
}
