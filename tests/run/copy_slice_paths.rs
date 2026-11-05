// Slice copies of `Copy` elements, which copy bytes at once, beside the
// element-by-element clones of other types: enums with payloads, tuples,
// zero-sized types, empty slices and overlapping ranges of one vector.

#[derive(Clone, Copy, Debug, PartialEq)]
enum Value {
    Unknown { int: bool, float: bool },
    Known(u32),
    Link(u64),
    Pair(u8, i16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Unit;

#[derive(Debug, PartialEq)]
struct Counted(u32);

static mut CLONES: u32 = 0;

impl Clone for Counted {
    fn clone(&self) -> Self {
        unsafe { CLONES += 1 };
        Counted(self.0 + 100)
    }
}

fn values() -> Vec<(Value, (u32, u8))> {
    let mut out = Vec::new();
    for i in 0..37u32 {
        let v = match i % 4 {
            0 => Value::Unknown { int: i % 3 == 0, float: i % 5 == 0 },
            1 => Value::Known(i * 7),
            2 => Value::Link(u64::MAX - i as u64),
            _ => Value::Pair(i as u8, -(i as i16)),
        };
        out.push((v, (i, (i * 3) as u8)));
    }
    out
}

fn main() {
    let source = values();

    let mut snapshot = Vec::new();
    snapshot.extend_from_slice(&source);
    snapshot.extend_from_slice(&source[5..9]);
    snapshot.extend_from_slice(&[]);
    println!("{} {:?} {:?}", snapshot.len(), snapshot[2], snapshot[39]);
    assert_eq!(&snapshot[..37], &source[..]);

    let copied = source.to_vec();
    let cloned = source.clone();
    println!("{} {}", copied == source, cloned == source);

    let mut within = vec![Value::Known(1), Value::Link(2), Value::Pair(3, -3)];
    within.extend_from_within(1..);
    within.extend_from_within(..2);
    within.extend_from_within(2..2);
    println!("{:?}", within);

    let mut target = [Value::Known(0); 4];
    target.copy_from_slice(&[Value::Link(9), Value::Known(8), Value::Pair(7, 7), Value::Link(6)]);
    println!("{:?}", target);
    target[1..3].clone_from_slice(&[Value::Unknown { int: true, float: false }, Value::Known(5)]);
    println!("{:?}", target);

    let units = vec![Unit; 5];
    let mut more = units.clone();
    more.extend_from_slice(&units);
    more.extend_from_within(1..4);
    let unit_copy = more.to_vec();
    println!("{} {} {:?}", more.len(), unit_copy.len(), unit_copy.first());
    let mut unit_target = [Unit; 3];
    unit_target.copy_from_slice(&[Unit, Unit, Unit]);
    unit_target.clone_from_slice(&more[..3]);

    let boxed: Box<[u16]> = vec![1u16, 2, 65535].into_boxed_slice();
    let boxed_copy = boxed.clone();
    println!("{:?}", boxed_copy);

    let mut text = String::from("copy");
    text.push_str(" paths");
    text.push_str("");
    text.push('é');
    let text_copy = text.clone();
    println!("{} {}", text_copy, text_copy.len());
    let bytes = text.as_bytes().to_vec();
    println!("{:?}", &bytes[..4]);

    let empty: Vec<u64> = Vec::new();
    let empty_copy = empty.clone();
    let mut grow = empty_copy.to_vec();
    grow.extend_from_slice(&empty);
    println!("{} {}", grow.len(), grow.capacity());

    // Types that are not `Copy` still clone each element in turn.
    let counted = vec![Counted(1), Counted(2), Counted(3)];
    let mut again = counted.clone();
    again.extend_from_slice(&counted[1..]);
    again.extend_from_within(..1);
    let mut slots = [Counted(0), Counted(0)];
    slots.clone_from_slice(&counted[..2]);
    let strings = vec![String::from("a"), String::from("bc")];
    let strings_copy = strings.to_vec();
    println!("{:?} {:?} {:?} {}", again, slots, strings_copy, unsafe { CLONES });

    let mut sum = 0u64;
    for (v, (i, b)) in snapshot.iter().chain(copied.iter()) {
        sum += *i as u64 + *b as u64;
        if let Value::Link(x) = v {
            sum = sum.wrapping_add(*x);
        }
    }
    std::process::exit((sum % 251) as i32);
}
