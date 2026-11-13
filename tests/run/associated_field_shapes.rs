trait Family { type Item; type Bytes; }
struct Provider;
impl Family for Provider { type Item = u64; type Bytes = [u8; 4]; }
type Item = <Provider as Family>::Item;
type Bytes = <Provider as Family>::Bytes;
struct Record { item: Item, flag: bool }
struct TwoWords { aliased: Item, ordinary: u64 }
struct Nested { lead: u8, record: Record, tail: Item }
struct Storage { bytes: Bytes, flag: bool, tail: Item }
struct Borrowed<'a> { value: &'a u8, item: Item, flag: bool }
enum Choice { Empty, Value(Item) }
fn make(item: Item) -> Record { Record { item, flag: true } }
fn storage(bytes: Bytes) -> Storage { Storage { bytes, flag: true, tail: 91 } }
fn borrowed<'a>(value: &'a u8, item: Item) -> Borrowed<'a> { Borrowed { value, item, flag: false } }
fn main() {
    let words = TwoWords { aliased: 11, ordinary: 29 };
    assert_eq!(words.aliased, 11);
    assert_eq!(words.ordinary, 29);
    let nested = Nested { lead: 3, record: make(0x123456789abcdef0), tail: 73 };
    let memory = storage([2, 4, 6, 8]);
    let input = 17;
    let reference = borrowed(&input, 101);
    let choice = Choice::Value(113);
    let number = match choice { Choice::Empty => 0, Choice::Value(item) => item };
    assert_eq!(nested.record.item, 0x123456789abcdef0);
    assert!(nested.record.flag);
    assert_eq!(memory.bytes, [2, 4, 6, 8]);
    assert!(memory.flag);
    assert!(!reference.flag);
    println!("{} {} {} {} {} {} {}", nested.lead, nested.tail, memory.bytes[3], memory.tail, *reference.value, reference.item, number);
    println!("{} {} {} {}", core::mem::size_of::<Record>(), core::mem::size_of::<Storage>(), words.aliased, words.ordinary);
}
