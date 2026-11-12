trait Lending { type Ref<'a> where Self:'a; }
struct Lender;
impl Lending for Lender { type Ref<'a>=&'a u32; }
fn lend<'a,T:Lending+'static>(f:for<'b>fn(&'b T)-><T as Lending>::Ref<'b>, value:&'a T)-><T as Lending>::Ref<'a>{f(value)}
fn lend_static(_: &Lender)->&u32 { &37 }
trait Family<'a>: 'static { type Output: 'a; }
struct Borrowed;
impl<'a> Family<'a> for Borrowed { type Output = &'a str; }
struct Pair;
impl<'a> Family<'a> for Pair { type Output = (&'a str, &'a str); }
struct Number;
impl<'a> Family<'a> for Number { type Output = usize; }
struct Holder<T>(T);
impl<'a,T:'static> Family<'a> for Holder<T> { type Output = T; }
struct Fixed;
impl<'a> Family<'a> for Fixed { type Output = &'static str; }
fn invoke<'a, Y: for<'s> Family<'s>>(
    callback: for<'s> fn(&'s str) -> <Y as Family<'s>>::Output, value: &'a str,
) -> <Y as Family<'a>>::Output { callback(value) }
fn identity(value: &str) -> &str { value }
fn pair(value: &str) -> (&str, &str) { (value, value) }
fn number(value: &str) -> usize { value.len() }
fn fixed(_: &str) -> &'static str { "fixed" }
fn main() {
    assert_eq!(*lend::<Lender>(lend_static, &Lender), 37);
    let owned = String::from("borrowed");
    assert_eq!(invoke::<Borrowed>(identity, &owned), "borrowed");
    let pointer: for<'a> fn(&'a str) -> &'a str = identity;
    assert_eq!(invoke::<Borrowed>(pointer, &owned), "borrowed");
    let inferred: for<'a> fn(&'a str) -> <Holder<_> as Family<'a>>::Output = number;
    assert_eq!(inferred(&owned), 8);
    assert_eq!(invoke::<Pair>(pair, &owned), ("borrowed", "borrowed"));
    assert_eq!(invoke::<Number>(number, &owned), 8);
    let output = invoke::<Fixed>(fixed, &owned);
    drop(owned);
    assert_eq!(output, "fixed");
    println!("associated pointer outputs passed");
}
