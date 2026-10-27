struct Layout(u32);
enum Kind<'a> {
    By(&'a Layout),
    Value,
}
fn classify<'a>(func: &u32, params: &'a [Layout], i: usize) -> Kind<'a> {
    if *func != 0 {
        Kind::By(&params[i])
    } else {
        Kind::Value
    }
}
fn collect<'s>(func: &u32, params: &'s [Layout], args: &[usize]) -> Option<&'s Layout> {
    let func = func;
    let kind = |i: usize| classify(func, params, i);
    let kinds = args.iter().map(|&i| kind(i));
    let mut result = None;
    for value in kinds {
        if let Kind::By(layout) = value {
            result = Some(layout);
        }
    }
    result
}
fn main() {
    let f = 1;
    let params = [Layout(7)];
    assert_eq!(collect(&f, &params, &[0]).unwrap().0, 7);
    println!("closure origin {}", collect(&f, &params, &[0]).unwrap().0);
}
