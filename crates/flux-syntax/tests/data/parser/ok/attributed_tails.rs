fn tail(x: u8) -> bool {
    #[allow(unused)]
    match x { 1 | 3 => true, _ => false }
}
fn closure(x: u8) -> bool {
    let f = || {
        #[allow(unused)]
        #[allow(unused_braces)]
        { x == 3 }
    };
    f()
}
fn statement(x: u8) -> bool {
    #[allow(unused)]
    match x { 1 => true, _ => false };
    #[allow(unused)]
    matches!(x, 1 | 3)
}
