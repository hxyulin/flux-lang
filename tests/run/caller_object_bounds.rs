use std::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
#[derive(Debug)]
struct ParseError(i64);
impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "bad value {}", self.0)
    }
}
impl std::error::Error for ParseError {}
impl Drop for ParseError {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
#[derive(Debug)]
struct Number(u8);
impl TryFrom<i64> for Number {
    type Error = ParseError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if (0..=255).contains(&value) {
            Ok(Number(value as u8))
        } else {
            Err(ParseError(value))
        }
    }
}
fn convert<T: TryFrom<i64>>(value: i64) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
where
    T::Error: std::error::Error + Send + Sync + 'static,
{
    let value: Result<T, _> = value.try_into();
    value.map_err(|err| err.into())
}
trait Parser<I, O> {
    fn parse(&mut self, input: &mut I) -> O;
    fn by_ref(&mut self) -> ByRef<'_, Self, I, O>
    where
        Self: Sized,
    {
        ByRef {
            p: self,
            marker: core::marker::PhantomData,
        }
    }
}
struct ByRef<'a, P, I, O> {
    p: &'a mut P,
    marker: core::marker::PhantomData<(I, O)>,
}
impl<P: Parser<I, O>, I, O> Parser<I, O> for ByRef<'_, P, I, O> {
    fn parse(&mut self, input: &mut I) -> O {
        self.p.parse(input)
    }
}
trait Modal<I, O>: Parser<I, O> {}
impl<I, O, P: Parser<I, O>> Modal<I, O> for P {}
struct Token;
impl<'a> Parser<&'a str, &'a str> for Token {
    fn parse(&mut self, input: &mut &'a str) -> &'a str {
        let (first, remaining) = input.split_at(1);
        *input = remaining;
        first
    }
}
fn wrap<'a>(mut parser: impl Modal<&'a str, &'a str>) -> impl FnMut(&mut &'a str) -> &'a str {
    move |input: &mut &'a str| parser.by_ref().parse(input)
}
fn forward<F: FnOnce()>(f: F) {
    f();
}
fn require_bound<'a, T: 'a>(_: core::marker::PhantomData<&'a mut &'a ()>, value: T) {
    drop(value);
}
fn bounded<'a, B, A: 'a>(first: B, value: A) {
    let tag: core::marker::PhantomData<&'a mut &'a ()> = core::marker::PhantomData;
    forward(move || {
        forward(move || require_bound(tag, value));
    });
    drop(first);
}
trait Typed: Clone + Send + Sync + 'static {
    type Value: Copy + Send + Sync + 'static;
}
trait AnyParser {
    fn width(&self) -> usize;
}
impl<T: Copy + Send + Sync + 'static, P: Typed<Value = T>> AnyParser for P {
    fn width(&self) -> usize {
        core::mem::size_of::<T>()
    }
}
#[derive(Clone)]
struct NumericParser;
impl Typed for NumericParser {
    type Value = u64;
}
fn erase_parser<P: Typed>(parser: P) -> Box<dyn AnyParser> {
    Box::new(parser)
}
trait Value {
    fn value(&self) -> u32;
}
trait BoxedValue {
    fn clone_box(&self) -> Box<dyn BoxedValue>;
    fn value(&self) -> u32;
}
impl<T: Value + Clone + 'static> BoxedValue for T {
    fn clone_box(&self) -> Box<dyn BoxedValue> {
        Box::new(self.clone())
    }
    fn value(&self) -> u32 {
        Value::value(self)
    }
}
#[derive(Clone)]
struct ClonedValue(u32);
impl Value for ClonedValue {
    fn value(&self) -> u32 {
        self.0
    }
}
unsafe fn disjoint<'a, T>(
    pointers: [Option<core::ptr::NonNull<T>>; 2],
    _: &'a mut [T; 2],
) -> [Option<&'a mut T>; 2] {
    pointers.map(|pointer| {
        pointer.map(|mut pointer| {
            // SAFETY: the caller supplies distinct pointers into the loaned array.
            unsafe { pointer.as_mut() }
        })
    })
}
fn run_scoped<T: Send>(value: T) {
    std::thread::scope(|scope| {
        scope.spawn(move || drop(value)).join().unwrap();
    });
}
fn main() {
    assert_eq!(convert::<Number>(7).unwrap().0, 7);
    let error = convert::<Number>(-3).unwrap_err();
    assert_eq!(error.to_string(), "bad value -3");
    assert_eq!(DROPS.load(Ordering::SeqCst), 0);
    println!("caller error {error}");
    drop(error);
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    let text = String::from("abc");
    bounded(String::from("owned"), text.as_str());
    let mut input = text.as_str();
    let mut parser = wrap(Token);
    let a = parser(&mut input);
    let b = parser(&mut input);
    assert_eq!((a, b, input), ("a", "b", "c"));
    assert_eq!(a.as_ptr(), text.as_ptr());
    println!(
        "caller parser {a} {b} {input} {}",
        DROPS.load(Ordering::SeqCst)
    );
    run_scoped(text.as_str());
    assert_eq!(erase_parser(NumericParser).width(), 8);
    let boxed: Box<dyn BoxedValue> = Box::new(ClonedValue(73));
    assert_eq!(boxed.clone_box().value(), 73);
    let mut values = [31u32, 47];
    let base = values.as_mut_ptr();
    // SAFETY: both pointers are within the two-element array and distinct.
    let pointers = unsafe {
        [
            core::ptr::NonNull::new(base),
            core::ptr::NonNull::new(base.add(1)),
        ]
    };
    // SAFETY: pointers name exactly the distinct elements of values.
    let [Some(a), Some(b)] = (unsafe { disjoint(pointers, &mut values) }) else {
        panic!()
    };
    *a += 1;
    *b += 2;
    assert_eq!(values, [32, 49]);
    println!("caller disjoint {} {}", values[0], values[1]);
}
