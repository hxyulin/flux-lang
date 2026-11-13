trait Select<'a> { type Out; }
struct S;
impl<'a> Select<'a> for S { type Out = u32; }
fn select<'a>(_: &'a u8) -> u32 {
    let value: <S as Select<'a>>::Out = 7;
    value
}
fn main() { println!("{}", select(&0)); }
