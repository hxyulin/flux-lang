fn patterns(x: Value) {
    let _ = x;
    let (a, mut b, ref c, ref mut d) = x;
    // Parsed, and reported later as unstable, as rustc does.
    let (mut ref e, mut ref mut f) = x;
    let Point { x, y: renamed, .. } = p;
    let [first, .., last] = arr;
    let [head, rest @ ..] = arr;
    let &(a, &mut b) = r;
    let Tuple(a, ..) = t;
    let S { 0: zero, 1: one } = s;
    match x {
        0 | 1 => {}
        2..=9 => {}
        ..0 => {}
        10.. => {}
        -5..=-1 => {}
        'a'..='z' | 'A'..='Z' => {}
        "str" | b"bytes" => {}
        true | false => {}
        Some(n @ 1..=3) if n > 1 => {}
        Enum::Variant { field: Some(_), .. } => {}
        CONST => {}
        <T as Trait>::ASSOC => {}
        matches!() => {}
        (..) => {}
        | Leading | Pipe => {}
    }
    let closure = |(a, b): (u8, u8), &c| a + b + c;
}
