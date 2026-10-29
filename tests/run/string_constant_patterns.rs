const FIELD: &str = "alpha";
type Text = &'static str;
const ALIAS: Text = FIELD;
const UNICODE: &str = concat!("hé", "\0", "🦀");
const fn chosen(flag: bool) -> &'static str {
    if flag { "alpha" } else { "beta" }
}
const CHOSEN: &str = chosen(false);
const EMPTY: &str = "";

fn classify(value: &str) -> u32 {
    match value {
        ALIAS => 1,
        CHOSEN => 2,
        UNICODE => 3,
        EMPTY => 4,
        _ => 0,
    }
}
fn nested(value: &Option<&str>) -> u32 {
    match value {
        Some(FIELD | CHOSEN) => 7,
        Some(UNICODE) => 8,
        _ => 0,
    }
}
fn explicit(value: &&str) -> u32 {
    match value {
        &FIELD => 9,
        _ => 0,
    }
}
fn guarded(value: &str, accept: bool) -> u32 {
    match value {
        FIELD if accept => 10,
        FIELD => 11,
        _ => 0,
    }
}
fn let_else(value: &str) -> bool {
    let FIELD = value else { return false };
    true
}
fn main() {
    let a = String::from("alpha");
    let b = String::from("beta");
    let u = String::from("hé\0🦀");
    let mut total = 0;
    for (input, expected) in [
        (a.as_str(), 1),
        (b.as_str(), 2),
        (u.as_str(), 3),
        ("", 4),
        ("alph", 0),
        ("alphab", 0),
    ] {
        assert_eq!(classify(input), expected);
        total += classify(input);
    }
    assert_eq!(nested(&Some(a.as_str())), 7);
    assert_eq!(nested(&Some(b.as_str())), 7);
    assert_eq!(nested(&Some(u.as_str())), 8);
    assert_eq!(nested(&None), 0);
    assert_eq!(explicit(&a.as_str()), 9);
    assert_eq!(explicit(&b.as_str()), 0);
    assert_eq!(guarded(a.as_str(), true), 10);
    assert_eq!(guarded(a.as_str(), false), 11);
    assert!(let_else(a.as_str()));
    assert!(!let_else(b.as_str()));
    println!("string patterns {total} 7 8 9 10 11");
}
