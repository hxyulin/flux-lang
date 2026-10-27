#[derive(Clone, Copy)]
struct Instruction {
    opcode: u32,
}
fn intervals(
    input: &[Instruction],
    is_call: &dyn Fn(&Instruction) -> bool,
    returns_twice: &dyn Fn(&Instruction) -> bool,
) -> usize {
    let mut result = 0;
    for inst in input {
        if is_call(inst) {
            result += 1;
        }
        if returns_twice(inst) {
            result += 10;
        }
    }
    result
}
fn allocate(
    input: &[Instruction],
    is_call: impl Fn(&Instruction) -> bool,
    returns_twice: impl Fn(&Instruction) -> bool,
) -> usize {
    intervals(input, &is_call, &returns_twice)
}
fn forwarding<F: Fn(&Instruction) -> bool>(input: &[Instruction], f: &F) -> usize {
    intervals(input, &f, &|_| false)
}
fn update<F: FnMut(&mut u32)>(mut f: F, x: &mut u32) {
    let call: &mut dyn FnMut(&mut u32) = &mut f;
    call(x);
    call(x);
}
fn boxed<F: Fn(&u32) -> bool + 'static>(f: F) -> Box<dyn Fn(&u32) -> bool> {
    Box::new(f)
}
fn main() {
    let input = [
        Instruction { opcode: 1 },
        Instruction { opcode: 2 },
        Instruction { opcode: 1 },
        Instruction { opcode: 3 },
    ];
    let call = 1;
    let twice = 3;
    let result = allocate(&input, |i| i.opcode == call, |i| i.opcode == twice);
    assert_eq!(result, 12);
    assert_eq!(forwarding(&input, &|i| i.opcode == call), 2);
    let mut calls = 0;
    let mut value = 5;
    update(
        |x| {
            *x += 3;
            calls += 1;
        },
        &mut value,
    );
    assert_eq!(value, 11);
    assert_eq!(calls, 2);
    let callback = boxed(|x| *x == 7);
    let borrowed: &dyn Fn(&u32) -> bool = &*callback;
    assert!(borrowed(&7));
    assert!(!borrowed(&8));
    println!("caller objects {} {} {}", result, value, calls);
}
