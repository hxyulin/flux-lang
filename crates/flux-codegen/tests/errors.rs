//! Errors found only when code is generated: constants whose values
//! depend on generic arguments are evaluated for the instances that use
//! them, so as in rustc, one that fails is reported then, at its value.

/// The errors lowering `text` to keel IR reports, as `line:column:
/// message`; the program must check cleanly before that.
fn lowering_errors(text: &str) -> Vec<String> {
    let (_db, _, hir) = flux_hir::lower_text("main.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:#?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:#?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:#?}", mir.diagnostics);
    let Err(errors) = flux_codegen::lower_module(&hir, &typed, &mir) else {
        return Vec::new();
    };
    errors
        .iter()
        .map(|d| {
            let before = &text[..d.range.start() as usize];
            let line = before.matches('\n').count() + 1;
            let column = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
            format!("{line}:{column}: {}", d.message)
        })
        .collect()
}

#[test]
fn trait_constant_fails_for_one_instance() {
    let text = "\
trait Tr {
    const A: u8;
    const B: u8 = Self::A + 200;
}
struct S;
struct T;
impl Tr for S {
    const A: u8 = 10;
}
impl Tr for T {
    const A: u8 = 100;
}
fn get<X: Tr>() -> u8 {
    X::B
}
fn main() {
    let _ = get::<S>();
    let _ = get::<T>();
}
";
    assert_eq!(
        lowering_errors(text),
        ["3:19: attempt to compute `100_u8 + 200_u8`, which would overflow"]
    );
}

#[test]
fn failure_is_reported_where_it_happens() {
    // `C` fails because `B` does: rustc reports `B`.
    let text = "\
struct W<T>(T);
trait Size {
    const N: u8;
}
impl Size for i32 {
    const N: u8 = 4;
}
impl<T: Size> W<T> {
    const B: u8 = T::N * 100;
    const C: u8 = Self::B + 1;
}
fn get<T: Size>() -> u8 {
    W::<T>::C
}
fn main() {
    let _ = get::<i32>();
}
";
    assert_eq!(
        lowering_errors(text),
        ["9:19: attempt to compute `4_u8 * 100_u8`, which would overflow"]
    );
}

#[test]
fn instances_only_dead_code_needs_are_not_evaluated() {
    // As in rustc, instances are found from `main`.
    let text = "\
trait Tr {
    const A: u8;
    const B: u8 = Self::A + 200;
}
struct T;
impl Tr for T {
    const A: u8 = 100;
}
fn get<X: Tr>() -> u8 {
    X::B
}
fn unused() -> u8 {
    get::<T>()
}
fn main() {}
";
    assert_eq!(lowering_errors(text), Vec::<String>::new());
}

#[test]
fn endless_instances_are_an_error() {
    let text = "\
struct W<T>(T);
impl<T> W<T> {
    const A: u32 = W::<W<T>>::A;
}
fn get<T>() -> u32 {
    W::<T>::A
}
fn main() {
    let _ = get::<u8>();
}
";
    assert_eq!(
        lowering_errors(text),
        ["3:20: queries overflow the depth limit!"]
    );
}

#[test]
fn unused_instances_are_not_evaluated() {
    let text = "\
trait Tr {
    const A: u8;
    const B: u8 = Self::A + 200;
}
struct T;
impl Tr for T {
    const A: u8 = 100;
}
fn main() {
    let _ = T::A;
}
";
    assert_eq!(lowering_errors(text), Vec::<String>::new());
}
