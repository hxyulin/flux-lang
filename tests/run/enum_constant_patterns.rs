#[derive(Clone, Copy, PartialEq)]
#[repr(u16)]
enum Kind {
    Empty = 7,
    Word(u32) = 31,
    Record { first: u32, second: bool } = 94,
    Array([Option<u32>; 2]) = 201,
}
const EMPTY: Kind = Kind::Empty;
const WORD: Kind = Kind::Word(13);
const RECORD: Kind = Kind::Record {
    first: 7,
    second: true,
};
const ARRAY: Kind = Kind::Array([Some(11), None]);
fn classify(x: Kind) -> u32 {
    match x {
        EMPTY => 1,
        WORD => 2,
        RECORD => 3,
        ARRAY => 4,
        _ => 5,
    }
}
#[derive(PartialEq)]
enum Flag {
    Off,
    On,
}
const OFF: Flag = Flag::Off;
const ON: Flag = Flag::On;
fn exhaustive(x: Flag) -> bool {
    match x {
        OFF => false,
        ON => true,
    }
}
struct Manual(u32);
impl PartialEq for Manual {
    fn eq(&self, _: &Self) -> bool {
        panic!("manual equality must not run")
    }
}
#[derive(PartialEq)]
enum Inactive {
    Good,
    Bad(Manual),
}
const GOOD: Inactive = Inactive::Good;
fn inactive(x: Inactive) -> bool {
    matches!(x, GOOD)
}
fn main() {
    let inputs = [
        Kind::Empty,
        Kind::Word(13),
        Kind::Word(12),
        Kind::Record {
            first: 7,
            second: true,
        },
        Kind::Record {
            first: 7,
            second: false,
        },
        Kind::Record {
            first: 8,
            second: true,
        },
        Kind::Array([Some(11), None]),
        Kind::Array([Some(12), None]),
        Kind::Array([None, Some(11)]),
    ];
    let expected = [1, 2, 5, 3, 5, 5, 4, 5, 5];
    let mut sum = 0;
    for (i, input) in inputs.into_iter().enumerate() {
        assert_eq!(classify(input), expected[i]);
        sum += classify(input);
    }
    assert!(!exhaustive(Flag::Off));
    assert!(exhaustive(Flag::On));
    assert!(inactive(Inactive::Good));
    assert!(!inactive(Inactive::Bad(Manual(13))));
    let Kind::Word(13) = WORD else { panic!("word") };
    println!("enum patterns {} {}", sum, exhaustive(Flag::On));
}
