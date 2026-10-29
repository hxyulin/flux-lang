static VALUE: u32 = 73;
trait Binders {
    fn unused<'a>();
    fn output<'a>() -> &'a u32;
    fn mixed<'a, 'b>(input: &'a u32) -> &'b u32;
    fn input(input: &u32) -> &u32;
}
struct S;
impl Binders for S {
    fn unused() {}
    fn output<'result>() -> &'result u32
    where
        'result: 'result,
    {
        &VALUE
    }
    fn mixed<'output, 'input>(_: &'input u32) -> &'output u32 {
        &VALUE
    }
    fn input<'a>(input: &'a u32) -> &'a u32 {
        input
    }
}
trait Owner<'a> {
    fn input(input: &u32) -> &u32;
}
impl<'a> Owner<'a> for S {
    fn input<'b>(input: &'b u32) -> &'b u32
    where
        'a: 'a,
    {
        input
    }
}
fn main() {
    S::unused();
    let result: &'static u32 = <S as Binders>::output();
    let input = 29;
    let other: &'static u32 = <S as Binders>::mixed(&input);
    assert_eq!(*result, 73);
    assert_eq!(*other, 73);
    assert_eq!(*<S as Binders>::input(&input), 29);
    assert_eq!(*<S as Owner>::input(&input), 29);
    println!("method binders {} {} {}", result, other, input);
}
