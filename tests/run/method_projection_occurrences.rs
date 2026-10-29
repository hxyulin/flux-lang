trait Stream {
    type Checkpoint;
}
struct TokenSlice<'a, T> {
    input: &'a [T],
}
struct Checkpoint<T>(T);
impl<'a, T> Stream for TokenSlice<'a, T> {
    type Checkpoint = Checkpoint<Self>;
}
trait Offset<Start = Self> {
    fn offset(&self, other: &Start) -> usize;
}
impl<T> Offset<<TokenSlice<'_, T> as Stream>::Checkpoint> for TokenSlice<'_, T> {
    fn offset(&self, other: &<TokenSlice<'_, T> as Stream>::Checkpoint) -> usize {
        other.0.input.len() - self.input.len()
    }
}
trait Pair<A, B> {
    fn lengths(&self, left: &A, right: &B) -> usize;
}
impl<'a, 'b, T> Pair<Checkpoint<TokenSlice<'a, T>>, Checkpoint<TokenSlice<'b, T>>> for () {
    fn lengths(
        &self,
        left: &<TokenSlice<'_, T> as Stream>::Checkpoint,
        right: &<TokenSlice<'_, T> as Stream>::Checkpoint,
    ) -> usize {
        left.0.input.len() * 10 + right.0.input.len()
    }
}
trait SliceStream {
    type Slice;
    fn take(&mut self) -> Self::Slice;
}
trait Parse<I: SliceStream, E> {
    fn parse(&mut self, input: &mut I) -> Result<I::Slice, E>;
}
impl<I: SliceStream, E> Parse<I, E> for () {
    fn parse(&mut self, input: &mut I) -> Result<<I as SliceStream>::Slice, E> {
        Ok(input.take())
    }
}
trait Capture<I: SliceStream, E> {
    fn capture(&mut self, input: &mut I) -> Result<(u32, I::Slice), E>;
}
impl<I: SliceStream, E> Capture<I, E> for () {
    fn capture(&mut self, input: &mut I) -> Result<(u32, <I as SliceStream>::Slice), E> {
        Ok((7, input.take()))
    }
}
struct Input<'a>(&'a str);
impl<'a> SliceStream for Input<'a> {
    type Slice = &'a str;
    fn take(&mut self) -> Self::Slice {
        self.0
    }
}
struct MutInput<'a>(Option<&'a mut u32>);
impl<'a> SliceStream for MutInput<'a> {
    type Slice = &'a mut u32;
    fn take(&mut self) -> Self::Slice {
        self.0.take().unwrap()
    }
}
trait Element {
    type Item;
    fn item(&self) -> Self::Item;
}
struct Atom(u32);
impl Element for Atom {
    type Item = u32;
    fn item(&self) -> u32 {
        self.0
    }
}
struct BorrowedPair<'a, T>(&'a u32, T);
impl<'a, T: Element> Element for BorrowedPair<'a, T> {
    type Item = (&'a u32, T::Item);
    fn item(&self) -> Self::Item {
        (self.0, self.1.item())
    }
}
trait Inspect<A> {
    fn inspect(&self, item: &A) -> u32;
}
impl<'a, T: Element<Item = u32>> Inspect<<BorrowedPair<'a, T> as Element>::Item>
    for BorrowedPair<'a, T>
{
    fn inspect(&self, item: &<BorrowedPair<'a, T> as Element>::Item) -> u32 {
        *item.0 + item.1
    }
}
trait Supply<A> {
    fn supply(&self) -> A;
}
impl<'a, T: Element> Supply<<BorrowedPair<'a, T> as Element>::Item> for BorrowedPair<'a, T> {
    fn supply(&self) -> <BorrowedPair<'a, T> as Element>::Item {
        (self.0, self.1.item())
    }
}
fn invoke<A, R, F: FnOnce(A) -> R>(function: F, argument: A) -> R {
    function(argument)
}
fn main() {
    let values = [1, 2, 3, 4, 5];
    let start = Checkpoint(TokenSlice { input: &values });
    let tail = TokenSlice {
        input: &values[2..],
    };
    let distance = tail.offset(&start);
    assert_eq!(distance, 2);
    let short = [7, 11];
    let other = Checkpoint(TokenSlice { input: &short });
    let pair = ().lengths(&start, &other);
    assert_eq!(pair, 52);
    let text = String::from("original input");
    let mut input = Input(text.as_str());
    let result: Result<&str, ()> = ().parse(&mut input);
    let text_result = result.unwrap();
    assert_eq!(text_result.as_ptr(), text.as_ptr());
    let captured: Result<(u32, &str), ()> = ().capture(&mut input);
    let captured = captured.unwrap();
    assert_eq!(captured.0, 7);
    assert_eq!(captured.1.as_ptr(), text.as_ptr());
    let mut number = 31;
    let mut input = MutInput(Some(&mut number));
    let result: Result<&mut u32, ()> = ().parse(&mut input);
    *result.unwrap() += 6;
    assert_eq!(number, 37);
    let value = 41;
    let partial = BorrowedPair(&value, Atom(43));
    let method_item = BorrowedPair::<Atom>::supply;
    let supplied = invoke(method_item, &partial);
    assert_eq!(supplied.0 as *const u32, &value as *const u32);
    assert_eq!(partial.inspect(&supplied), 84);
    println!(
        "method projection {distance} {pair} {} {} {number} {}",
        text_result.len(),
        captured.0,
        supplied.1
    );
}
