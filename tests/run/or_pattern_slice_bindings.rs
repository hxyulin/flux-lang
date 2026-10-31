fn length(input: &[u8]) -> usize {
    match (input.len(), input) {
        (1, s) | (3, [0, s @ .., 0]) => s.len(),
        _ => 0,
    }
}

fn guarded(input: &[u8]) -> usize {
    match (input.len(), input) {
        (1, s) | (3, [0, s @ .., 0]) if s[0] == 9 => s.len() + 10,
        _ => 0,
    }
}

fn main() {
    for input in [&[8][..], &[0, 9, 0][..], &[0, 8, 0][..], &[][..]] {
        println!("{} {}", length(input), guarded(input));
    }
    // The implicit borrow may also occur in the first alternative.
    let input = &[0u8, 9, 0][..];
    let n = match (input.len(), input) {
        (3, [0, s @ .., 0]) | (1, s) => s.len(),
        _ => 0,
    };
    println!("{n}");
    let closure = || match (input.len(), input) {
        (1, s) | (3, [0, s @ .., 0]) => s[0],
        _ => 0,
    };
    println!("{}", closure());
    let mut bytes = [0u8, 9, 0];
    let slice = &mut bytes[..];
    match (slice.len(), slice) {
        (1, s) | (3, [0, s @ .., 0]) => s[0] += 1,
        _ => {}
    }
    println!("{}", bytes[1]);
}
