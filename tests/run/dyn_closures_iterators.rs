//! Closures and iterators through trait objects: `&dyn Fn` and `&mut
//! dyn FnMut` called like functions, arrays of them, `&mut dyn
//! Iterator<Item = u32>` driven by `next`, `for` loops and adapters
//! (through `core`'s `impl Iterator for &mut I` with `I: ?Sized`), and
//! `size_hint` through a vtable.

unsafe extern "C" { fn printf(format: *const i8, ...) -> i32; }

fn apply(f: &dyn Fn(i32) -> i32, x: i32) -> i32 { f(x) + f(x + 1) }
fn apply_mut(f: &mut dyn FnMut(i32), n: i32) { for i in 0..n { f(i); } }
fn sum_iter(it: &mut dyn Iterator<Item = u32>) -> u32 {
    let mut s = 0;
    while let Some(x) = it.next() { s += x; }
    s
}
fn first_two(it: &mut dyn Iterator<Item = u32>) -> u32 {
    let a = it.next().unwrap_or(0);
    let b = it.next().unwrap_or(0);
    a * 10 + b
}
fn double(x: i32) -> i32 { x * 2 }

fn main() {
    more();
    let k = 3;
    let add = |x: i32| x + k;
    let mut total = 0;
    {
        let mut acc = |i: i32| total += i;
        apply_mut(&mut acc, 5);
    }
    let fs: [&dyn Fn(i32) -> i32; 2] = [&add, &double];
    let mut r = 0..5u32;
    let mut arr = [7u32, 8, 9].into_iter();
    unsafe {
        printf(c"%d %d %d %d\n".as_ptr(), apply(&add, 1), apply(&double, 2), total, fs[0](1) + fs[1](1));
        printf(c"%u %u %u\n".as_ptr(), first_two(&mut r), sum_iter(&mut r), sum_iter(&mut arr));
    }
}


fn walk(it: &mut dyn Iterator<Item = u32>) -> u32 {
    let mut s = 0;
    for x in it { s = s * 10 + x; }
    s
}
fn mapped(it: &mut dyn Iterator<Item = u32>) -> u32 {
    it.map(|x| x * 2).filter(|x| *x > 2).sum()
}
fn hint(it: &dyn Iterator<Item = u32>) -> usize { it.size_hint().0 }
fn pick<'a>(fs: &[&'a dyn Fn(i32) -> i32], i: usize) -> &'a dyn Fn(i32) -> i32 { fs[i] }

fn more() {
    let mut a = [1u32, 2, 3].into_iter();
    let mut r = 0..4u32;
    let h = hint(&(5..9u32));
    let one = |x: i32| x + 1;
    let two = |x: i32| x * 2;
    let fs: [&dyn Fn(i32) -> i32; 2] = [&one, &two];
    let f = pick(&fs, 1);
    unsafe {
        printf(c"%u %u %zu %d\n".as_ptr(), walk(&mut a), mapped(&mut r), h, f(21));
    }
}
