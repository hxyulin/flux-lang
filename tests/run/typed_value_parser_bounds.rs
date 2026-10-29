use std::ops::{Bound, RangeBounds};
#[derive(Clone)]
struct RangedParser<T: TryFrom<i64> + Clone + Send + Sync> {
    bounds: (Bound<i64>, Bound<i64>),
    marker: core::marker::PhantomData<T>,
}
impl<T: TryFrom<i64> + Clone + Send + Sync> RangedParser<T> {
    fn range<R: RangeBounds<i64>>(range: R) -> Self {
        Self {
            bounds: (range.start_bound().cloned(), range.end_bound().cloned()),
            marker: core::marker::PhantomData,
        }
    }
}
trait TypedValueParser: Clone + Send + Sync + 'static {
    type Value: Clone + Send + Sync;
    fn parse(&self, n: i64) -> Result<Self::Value, String>;
}
impl<T: TryFrom<i64> + Clone + Send + Sync + 'static> TypedValueParser for RangedParser<T>
where
    T::Error: Send + Sync + 'static + std::error::Error + ToString,
{
    type Value = T;
    fn parse(&self, n: i64) -> Result<T, String> {
        if !self.bounds.contains(&n) {
            return Err(String::from("outside range"));
        }
        T::try_from(n).map_err(|e| e.to_string())
    }
}
fn parse_twice<P: TypedValueParser>(parser: P, n: i64) -> Result<P::Value, String> {
    let cloned = parser.clone();
    let _ = parser.parse(n)?;
    cloned.parse(n)
}
fn main() {
    let a = 2i64;
    let b = 19i64;
    let parser = RangedParser::<u8>::range(&a..=&b);
    assert_eq!(parse_twice(parser.clone(), 7).unwrap(), 7);
    assert_eq!(parser.parse(20).unwrap_err(), "outside range");
    let same = RangedParser::<i64>::range(..);
    assert_eq!(parse_twice(same, 37).unwrap(), 37);
    let overflow = RangedParser::<u8>::range(..);
    assert_eq!(
        overflow.parse(300).unwrap_err(),
        "number too large to fit in target type"
    );
    let joined = std::thread::scope(|scope| {
        scope
            .spawn(move || parse_twice(parser, 11).unwrap())
            .join()
            .unwrap()
    });
    assert_eq!(joined, 11);
    println!("typed parser {} {}", joined, 7);
}
