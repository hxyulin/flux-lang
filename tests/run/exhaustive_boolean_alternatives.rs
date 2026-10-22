//! Independent exhaustive alternatives are one proof, rather than 2^24 proofs.
unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}
fn exhaustive(value: [bool; 24]) -> i32 {
    match value {
        [
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
            true | false,
        ] => 7,
    }
}
fn classify(value: (bool, bool)) -> i32 {
    match value {
        (true | false, true) => 11,
        (true, false) => 13,
        (false, false) => 17,
    }
}
fn guarded(value: (bool, bool), calls: &mut i32) -> i32 {
    match value {
        (true | false, true)
            if {
                *calls += 1;
                false
            } =>
        {
            19
        }
        (true | true, false)
            if {
                *calls += 1;
                *calls == 2
            } =>
        {
            23
        }
        _ => 29,
    }
}
struct S(i32);
fn moved(value: (bool, S), pass: bool) -> i32 {
    match value {
        (true | false, s) if pass => s.0,
        (_, s) => s.0 + 1,
    }
}
fn borrowed(value: &mut (bool, i32)) -> i32 {
    match value {
        (true | false, n) => {
            *n += 1;
            *n
        }
    }
}
fn indirect(value: &&bool) -> i32 {
    match value {
        false | true if **value => 43,
        _ => 47,
    }
}
fn borrowed_field(value: (&bool, bool)) -> i32 {
    match value {
        (true | false, true) => 53,
        (_, false) => 59,
    }
}
fn main() {
    let mut calls = 0;
    let a = guarded((true, true), &mut calls);
    let first = calls;
    calls = 0;
    let b = guarded((true, false), &mut calls);
    let mut pair = (false, 31);
    let yes = true;
    let no = false;
    unsafe {
        printf(
            c"%d %d %d %d\n".as_ptr(),
            indirect(&&yes),
            indirect(&&no),
            borrowed_field((&yes, true)),
            borrowed_field((&no, false)),
        );
        printf(
            c"%d %d %d %d %d %d %d %d\n".as_ptr(),
            a,
            first,
            b,
            calls,
            moved((true, S(37)), false),
            moved((false, S(41)), true),
            borrowed(&mut pair),
            pair.1,
        );
        printf(
            c"%d %d %d %d %d\n".as_ptr(),
            exhaustive([true; 24]),
            exhaustive([false; 24]),
            classify((true, true)),
            classify((true, false)),
            classify((false, false)),
        );
    }
}
