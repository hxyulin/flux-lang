#[derive(Clone, Copy)]
struct Value(u32);
struct Inst { results: Vec<Value> }
fn single(inst: &Inst) -> u32 {
    let [result] = inst.results[..] else { return 0 };
    result.0
}
fn main() {
    let mut sum = 0;
    for len in 0..13 {
        let inst = Inst { results: (0..len).map(|i| Value(i + 29)).collect() };
        sum += single(&inst);
    }
    assert_eq!(sum, 29);
    let words = vec![String::from("one"), String::from("two"), String::from("three")];
    let [ref first, ref rest @ ..] = words[..] else { return };
    assert_eq!(first, "one");
    assert_eq!(rest.len(), 2);
    let mut values = vec![7u32, 11, 29];
    let [ref mut head, ref mut tail @ ..] = values[..] else { return };
    *head += 5;
    tail[0] += 7;
    assert_eq!(values.as_slice(), &[12, 18, 29]);
    println!("slice places {} {} {}", sum, words.len(), values[1]);
}
