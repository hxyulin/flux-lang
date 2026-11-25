#[inline(never)]
fn search_stack(states: &mut Vec<i16>, start: i16, count: i16) -> i64 {
    let mut result = 0;
    for offset in 0..count {
        states.push(start.wrapping_add(offset));
        if offset % 4 == 3 {
            result += i64::from(*states.last().expect("three pushes filled the stack"));
            states.truncate(states.len().saturating_sub(2));
        }
    }
    result + states.iter().copied().map(i64::from).sum::<i64>()
}

union EntryValue {
    value: u32,
}

struct Entry {
    data: EntryValue,
    tag: u32,
}

#[inline(never)]
fn push_entry(entries: &mut Vec<Entry>, value: u32, tag: u32) {
    entries.push(Entry {
        data: EntryValue { value },
        tag,
    });
}

fn main() {
    let mut states = Vec::with_capacity(1);
    assert_eq!(states.last(), None);
    let sum = search_stack(&mut states, -24, 57);
    let last = *states.last().unwrap();
    let len = states.len();
    states.truncate(len + 10);
    assert_eq!(states.len(), len);
    states.truncate(3);
    println!("{sum} {last} {len} {:?}", states.as_slice());

    let mut signed = Vec::<i8>::with_capacity(2);
    for value in [-128, -7, 0, 9, 127] {
        signed.push(value);
    }
    println!("{signed:?}");

    let x = String::from("ownership");
    let mut pointers = Vec::with_capacity(1);
    pointers.push(&x);
    pointers.push(&x);
    println!("{} {}", pointers[0], pointers[1]);

    let mut entries = Vec::with_capacity(2);
    push_entry(&mut entries, 11, 29);
    println!("{} {}", unsafe { entries[0].data.value }, entries[0].tag);

    let mut units = Vec::<()>::new();
    units.push(());
    units.push(());
    println!("{}", units.len());
}
