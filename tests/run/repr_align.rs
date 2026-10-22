//! Aligned aggregates retain their layout and address alignment in all storage.
#![allow(dead_code)]

#[repr(C, align(64,))]
#[derive(Clone, Copy)]
struct Block([u8; 3]);
#[repr(align(32))]
#[repr(align(8))]
struct Generic<T>(u8, T, bool);
#[repr(align(128))]
#[derive(Clone, Copy)]
struct Empty;
#[repr(C)]
struct Nested(u8, Block, u16, Empty);
#[repr(align(32))]
enum Tagged {
    A(u8),
    B(u32),
}
#[repr(C, align(64))]
enum CTagged {
    A(u8),
    B(u32),
}
#[repr(u8, align(32))]
enum IntTagged {
    A(u8),
    B(u32),
}
#[repr(align(64))]
enum Niche {
    A(&'static u8),
    B,
}
#[repr(align(64))]
enum Single {
    A(u8),
}
// The other variant must fit before or after the niche with its padded size.
#[repr(align(2))]
enum PaddedNiche {
    A(u8, bool),
    B(bool),
    C,
}
#[repr(align(2))]
enum PaddedArrayNiche {
    A([u8; 3], bool),
    B([u8; 3]),
    C,
}
enum Never {}
#[repr(align(536870912))]
struct Maximum;

static BLOCK: Block = Block([11, 22, 33]);
static BLOCKS: [Block; 2] = [Block([1, 2, 3]), Block([4, 5, 6])];
static EMPTY: Empty = Empty;
static mut MUTABLE: Block = Block([4, 5, 6]);
static NUMBER: u8 = 17;
static NICHE: Niche = Niche::A(&NUMBER);
const LAYOUT: (usize, usize) = (size_of::<Nested>(), align_of::<Nested>());

fn layout<T>() {
    println!("{} {}", size_of::<T>(), align_of::<T>());
}
fn aligned<T>(v: &T) {
    println!("{}", (v as *const T as usize) % align_of::<T>());
}
fn make(x: u8) -> Block {
    Block([x, x + 1, x + 2])
}
fn receive(v: Block) -> Block {
    aligned(&v);
    make(v.0[1])
}
fn single(v: Single) -> Single {
    aligned(&v);
    v
}
fn generic<T>(v: T) -> Generic<T> {
    Generic(9, v, true)
}
fn main() {
    layout::<PaddedNiche>();
    layout::<Option<PaddedNiche>>();
    layout::<PaddedArrayNiche>();
    layout::<Block>();
    layout::<[Block; 3]>();
    layout::<Empty>();
    layout::<[Empty; 3]>();
    layout::<[Block; 0]>();
    layout::<Generic<u16>>();
    layout::<Generic<Block>>();
    layout::<Nested>();
    layout::<Tagged>();
    layout::<CTagged>();
    layout::<IntTagged>();
    layout::<Niche>();
    layout::<Option<Niche>>();
    layout::<Single>();
    layout::<Never>();
    layout::<Maximum>();
    println!("{} {}", LAYOUT.0, LAYOUT.1);
    aligned(&BLOCK);
    aligned(&BLOCKS[1]);
    aligned(&EMPTY);
    aligned(&NICHE);
    unsafe {
        let p = &raw mut MUTABLE;
        println!("{}", (p as usize) % 64);
        (*p).0[2] = 88;
        println!("{}", (*p).0[2]);
    }
    let tag = Tagged::B(1234);
    aligned(&tag);
    match tag {
        Tagged::A(x) => println!("{}", x),
        Tagged::B(x) => println!("{}", x),
    }
    let ct = CTagged::A(98);
    aligned(&ct);
    match ct {
        CTagged::A(x) => println!("{}", x),
        CTagged::B(x) => println!("{}", x),
    }
    let it = IntTagged::B(12345);
    aligned(&it);
    match it {
        IntTagged::A(x) => println!("{}", x),
        IntTagged::B(x) => println!("{}", x),
    }
    match &NICHE {
        Niche::A(x) => println!("{}", **x),
        Niche::B => println!("none"),
    }
    let none = Niche::B;
    aligned(&none);
    match none {
        Niche::A(x) => println!("{}", *x),
        Niche::B => println!("none"),
    }
    let a = receive(make(41));
    aligned(&a);
    println!("{:?}", a.0);
    let array = [make(7), make(8), make(9)];
    aligned(&array);
    aligned(&array[1]);
    println!(
        "{}",
        &array[1] as *const Block as usize - &array[0] as *const Block as usize
    );
    let nested = Nested(3, make(10), 999, Empty);
    aligned(&nested);
    aligned(&nested.1);
    aligned(&nested.3);
    println!(
        "{} {} {} {}",
        &nested.0 as *const u8 as usize - &nested as *const Nested as usize,
        &nested.1 as *const Block as usize - &nested as *const Nested as usize,
        &nested.2 as *const u16 as usize - &nested as *const Nested as usize,
        &nested.3 as *const Empty as usize - &nested as *const Nested as usize
    );
    let g = generic(make(19));
    aligned(&g);
    aligned(&g.1);
    let s = single(Single::A(77));
    aligned(&s);
    match s {
        Single::A(x) => println!("{}", x),
    }
    let heap = Box::new(receive(make(90)));
    aligned(&*heap);
    println!("{:?}", heap.0);
    let zst = Box::new(Empty);
    aligned(&*zst);
    let mut vec = Vec::new();
    vec.push(make(1));
    vec.push(make(2));
    vec.push(make(3));
    aligned(&vec[0]);
    aligned(&vec[2]);
    println!("{:?}", vec[2].0);
    let values: &mut [u32] = &mut [5, 6, 7];
    unsafe {
        *values.get_unchecked_mut(1usize) = 42;
    }
    println!("{} {}", unsafe { *values.get_unchecked(1usize) }, values[2]);
    let empty = [Empty, Empty];
    aligned(unsafe { empty.get_unchecked(1usize) });
}
