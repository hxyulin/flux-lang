#[derive(Debug)]
struct Value(u32);
impl From<u32> for Value {
    fn from(n: u32) -> Self {
        Value(n)
    }
}
#[derive(Debug)]
enum Item {
    Value(Value),
}
impl<V: Into<Value>> From<V> for Item {
    fn from(v: V) -> Self {
        Item::Value(v.into())
    }
}
enum Reset<T> {
    Value(T),
    Empty,
}
trait Convert<T> {
    fn convert(self) -> Reset<T>;
}
impl<T> Convert<T> for Reset<T> {
    fn convert(self) -> Reset<T> {
        self
    }
}
impl<I: Into<Value>> Convert<Value> for I {
    fn convert(self) -> Reset<Value> {
        Reset::Value(self.into())
    }
}
trait Parse {
    fn parse(&self) -> u32;
}
impl<F: Fn() -> u32> Parse for F {
    fn parse(&self) -> u32 {
        self()
    }
}
struct Parser(u32);
impl<P: Parse> From<P> for Parser {
    fn from(p: P) -> Self {
        Parser(p.parse())
    }
}
struct Values(Vec<u32>);
impl<I, T> From<I> for Values
where
    I: IntoIterator<Item = T>,
    T: Into<u32>,
{
    fn from(input: I) -> Self {
        Values(input.into_iter().map(|t| t.into()).collect())
    }
}
struct RangeParser<T> {
    bounds: (core::ops::Bound<i64>, core::ops::Bound<i64>),
    marker: core::marker::PhantomData<T>,
}
impl<T, B: core::ops::RangeBounds<i64>> From<B> for RangeParser<T> {
    fn from(range: B) -> Self {
        fn copied(value: core::ops::Bound<&i64>) -> core::ops::Bound<i64> {
            match value {
                core::ops::Bound::Included(v) => core::ops::Bound::Included(*v),
                core::ops::Bound::Excluded(v) => core::ops::Bound::Excluded(*v),
                core::ops::Bound::Unbounded => core::ops::Bound::Unbounded,
            }
        }
        RangeParser {
            bounds: (copied(range.start_bound()), copied(range.end_bound())),
            marker: core::marker::PhantomData,
        }
    }
}
trait Stream {
    type Checkpoint;
}
struct Checkpoint<I, S>(I, core::marker::PhantomData<S>);
struct StreamValue<T>(T);
impl<T> Stream for StreamValue<T> {
    type Checkpoint = Checkpoint<T, Self>;
}
trait Offset<Start = Self> {
    fn offset(&self, start: &Start) -> usize;
}
impl<T> Offset for StreamValue<T> {
    fn offset(&self, _: &Self) -> usize {
        11
    }
}
impl<T> Offset<<StreamValue<T> as Stream>::Checkpoint> for StreamValue<T> {
    fn offset(&self, _: &Checkpoint<T, Self>) -> usize {
        23
    }
}
fn main() {
    let item: Item = 17u32.into();
    let Item::Value(Value(n)) = item;
    assert_eq!(n, 17);
    let a = 29u32.convert();
    let Reset::Value(Value(n)) = a else { panic!() };
    assert_eq!(n, 29);
    let a: Reset<Value> = Reset::Value(Value(31));
    let a = a.convert();
    let Reset::Value(Value(n)) = a else { panic!() };
    assert_eq!(n, 31);
    let parser: Parser = (|| 43u32).into();
    assert_eq!(parser.0, 43);
    let values: Values = [1u8, 2, 7].into();
    assert_eq!(values.0, [1u32, 2, 7]);
    let empty: Reset<Value> = Reset::Empty;
    assert!(matches!(empty.convert(), Reset::Empty));
    let ranged: RangeParser<u8> = (7i64..=19).into();
    assert!(matches!(ranged.bounds.0, core::ops::Bound::Included(7)));
    assert!(matches!(ranged.bounds.1, core::ops::Bound::Included(19)));
    let unsigned: RangeParser<u64> = (2i64..9).into();
    assert!(matches!(unsigned.bounds.1, core::ops::Bound::Excluded(9)));
    let value = StreamValue(17u8);
    let checkpoint = Checkpoint(17u8, core::marker::PhantomData::<StreamValue<u8>>);
    assert_eq!(value.offset(&value), 11);
    assert_eq!(value.offset(&checkpoint), 23);
    println!("coherence {} {} {}", parser.0, values.0.len(), n);
}
