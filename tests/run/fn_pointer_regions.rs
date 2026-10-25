use std::cell::RefCell;
struct Pool { words: Vec<Vec<u32>>, bytes: Vec<Vec<u8>> }
fn word_lists(pool: &mut Pool) -> &mut Vec<Vec<u32>> { &mut pool.words }
fn take<T>(pool: &RefCell<Pool>, list: fn(&mut Pool) -> &mut Vec<Vec<T>>) -> Vec<T> {
    list(&mut pool.borrow_mut()).pop().unwrap_or_default()
}
fn give<T>(pool: &RefCell<Pool>, list: fn(&mut Pool) -> &mut Vec<Vec<T>>, value: Vec<T>) {
    list(&mut pool.borrow_mut()).push(value);
}
fn left<'a, 'b>(a: &'a u32, _: &'b u32) -> &'a u32 { a }
fn nested<'a, 'b>(value: &'b mut &'a u32) -> &'b mut &'a u32 { value }
fn id(value: &u32) -> &u32 { value }
fn some(value: &u32) -> Option<&u32> { Some(value) }
static FIXED: u32 = 73;
fn fixed() -> &'static u32 { &FIXED }
fn call_free<'a>(f: fn(&'a u32) -> &'a u32, value: &'a u32) -> &'a u32 { f(value) }
fn specialize<'a>(value: &'a fn(&u32) -> &u32) -> &'a fn(&'static u32) -> &'static u32 { value }
fn specialize_tuple(value: (fn(&u32) -> &u32, u32)) -> (fn(&'static u32) -> &'static u32, u32) { value }
trait InputContract { fn call(value: fn(&u32) -> &u32) -> u32; }
struct Specialize;
impl InputContract for Specialize {
    fn call(value: fn(&'static u32) -> &'static u32) -> u32 { *value(&FIXED) }
}
fn main() {
    let pool = RefCell::new(Pool { words: vec![vec![7u32, 11]], bytes: vec![vec![3u8, 5]] });
    let mut sum = 0;
    for _ in 0..37 {
        let mut words = take(&pool, word_lists);
        let bytes = take(&pool, |pool| &mut pool.bytes);
        sum += words[0] + u32::from(bytes[1]);
        words[0] += 1;
        give(&pool, word_lists, words);
        give(&pool, |pool| &mut pool.bytes, bytes);
    }
    assert_eq!(sum, 1110);
    let select: for<'a, 'b> fn(&'a u32, &'b u32) -> &'a u32 = left;
    let a = 29; let b = 31;
    assert_eq!(*select(&a, &b), 29);
    let edit: for<'a, 'b> fn(&'b mut &'a u32) -> &'b mut &'a u32 = nested;
    let mut value = &a;
    *edit(&mut value) = &b;
    assert_eq!(*value, 31);
    let closure: fn(&u32) -> &u32 = |value| value;
    assert_eq!(*closure(&a), 29);
    assert_eq!(*call_free(id, &b), 31);
    let optional: fn(&u32) -> Option<&u32> = some;
    assert_eq!(*optional(&a).unwrap(), 29);
    let constant: fn() -> &'static u32 = fixed;
    assert_eq!(*constant(), 73);
    let identity: for<'unused, 'a> fn(&'a u32) -> &'a u32 = id;
    let borrowed = specialize(&identity);
    let tuple = specialize_tuple((identity, 19));
    assert_eq!(*borrowed(&FIXED) + *tuple.0(&FIXED) + tuple.1, 165);
    assert_eq!(Specialize::call(identity), 73);
    println!("pointer regions {} {} {} {}", sum, value, constant(), tuple.1);
}
