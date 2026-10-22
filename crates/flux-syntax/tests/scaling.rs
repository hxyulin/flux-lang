//! Parsing takes time linear in the input: four times as many items, as
//! long a body, as deep a nesting or as long an expression takes about
//! four times as long.

#[path = "support/scaling.rs"]
mod scaling;

use scaling::{assert_scales, repeat};

fn parse(text: &str) {
    let parse = flux_syntax::parse_source_file(text);
    assert!(parse.errors().is_empty(), "{:?}", &parse.errors()[..1]);
}

#[test]
fn many_items() {
    let make = |n| {
        repeat(n, |i| {
            format!(
                "#[derive(Clone, Copy)]\nstruct S{i} {{ a: u32, b: (i64, bool) }}\n\
                 impl S{i} {{ fn get(&self) -> u32 {{ self.a + {i} }} }}\n\
                 fn f{i}(x: S{i}) -> u32 {{ x.get() * 2 }}"
            )
        })
    };
    assert_scales("items", 1000, make, |text| parse(text));
}

#[test]
fn long_body() {
    let make = |n| {
        let body = repeat(n, |i| {
            format!("    let x{i} = if x > {i} {{ x - 1 }} else {{ x + 1 }};")
        });
        format!("fn f(x: i32) {{\n{body}}}\n")
    };
    assert_scales("statements in a body", 2000, make, |text| parse(text));
}

#[test]
fn deep_nesting() {
    let make = |n| {
        let open = "{ ( [".repeat(n);
        let close = "] ) }".repeat(n);
        format!("fn f() -> i32 {{ {open} 1 {close} }}\n")
    };
    assert_scales("nested expressions", 100, make, |text| parse(text));
}

#[test]
fn long_expressions() {
    let make = |n| {
        let sum = repeat(n, |i| format!("    + a.b({i}).c[{i}] * {i}"));
        format!("fn f() -> i32 {{\n    0\n{sum}}}\n")
    };
    assert_scales("terms in an expression", 2000, make, |text| parse(text));
}
