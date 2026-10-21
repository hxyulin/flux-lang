fn precedence() {
    a = b = c;
    x += 1 * 2 + 3 << 4 & 5 ^ 6 | 7 == 8 && 9 || 10;
    y = -a.b().c[0]? as u8 as u32;
    z = !*&mut &&x;
    r = a..b;
    r = ..=b;
    r = a..;
    r = ..;
    f = x <= y && x >= z && x != w && x < v && x > u;
    s = x >> 2 >= 1 && x << 1 <= 2;
    c = x as usize <= y as usize;
}
fn postfix() {
    t.0.1;
    v.iter().map(|x| x * 2).collect::<Vec<_>>();
    Vec::<u8>::with_capacity(3);
    <Vec<u8> as Default>::default();
    fut.await;
    s.field;
}
fn literals() {
    (1, 2.5, 1e10, 0xff_u8, 'c', b'b', "s", b"b", c"c", r#"raw"#, true);
    [1, 2, 3];
    [0; 16];
    ();
    (1,);
    (1);
}
fn control() {
    if a { b } else if c { d } else { e }
    if let Some(x) = y && x > 0 && let Ok(z) = w { }
    while let Some(x) = it.next() { }
    'outer: for i in 0..10 { loop { break 'outer; } continue; }
    let v = loop { break 5; };
    let b = 'block: { break 'block 1; };
    match x { A => 1, B => { 2 } C => 3 }
    return;
}
fn structs() {
    let p = Point { x: 1, y };
    let q = Point { x: 2, ..p };
    let t = Tuple { 0: 1 };
    if p == (Point { x: 1, y: 2 }) {}
    if x == S {}
    match S {} {}
    for x in v {}
}
fn closures() {
    let a = || 1;
    let b = move |x: u8| -> u8 { x };
    let c = |x| |y| x + y;
    let d = async move { 1 };
    let e = async || 2;
    let f = unsafe { g() };
    let h = const { 1 + 1 };
    _ = discard();
}
fn attributes() {
    let v = [1, #[cfg(x)] 2];
    #[allow(unused)]
    let x = 1;
    #[rustfmt::skip]
    call();
}
