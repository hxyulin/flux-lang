//! Closures passed to generic functions that return or keep references
//! into their parameters, or capture references their results keep:
//! the borrow checker relates them through the callee's `Fn`-family
//! bounds, as rustc relates closure signatures.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

enum Slot {
    Empty,
    Full(i32),
}

struct Item {
    name: &'static str,
    qty: u32,
}

#[derive(Clone, Copy)]
enum Error<'a> {
    Bad { key: &'a str, code: i32 },
}

fn check<'a>(key: &'a str, r: Result<i32, i32>) -> Result<i32, Error<'a>> {
    r.map_err(|code| Error::Bad { key, code })
}

fn names<'a>(items: &'a [Item]) -> impl Iterator<Item = &'static str> + 'a {
    items.iter().map(|i| i.name)
}

fn quantities<'a>(items: &'a [Item]) -> impl Iterator<Item = &'a u32> + 'a {
    items.iter().map(|i| &i.qty)
}

fn full(slots: &[Slot]) -> impl Iterator<Item = &i32> + '_ {
    slots.iter().filter_map(|s| match s {
        Slot::Full(v) => Some(v),
        Slot::Empty => None,
    })
}

fn largest<'a>(items: &'a [Item]) -> Option<&'a Item> {
    items.iter().fold(None::<&Item>, |best, i| match best {
        Some(b) if b.qty >= i.qty => Some(b),
        _ => Some(i),
    })
}

fn main() {
    let slots = [Slot::Full(5), Slot::Empty, Slot::Full(7)];
    let total: i32 = slots
        .iter()
        .filter_map(|s| match s {
            Slot::Full(v) => Some(v),
            Slot::Empty => None,
        })
        .sum();
    show(1, total);
    show(2, full(&slots).count() as i32);

    let items = [
        Item { name: "hammer", qty: 1 },
        Item { name: "saw", qty: 4 },
        Item { name: "drill", qty: 2 },
    ];
    let lens: usize = names(&items).map(|n| n.len()).sum();
    show(3, lens as i32);
    let qty: u32 = quantities(&items).copied().sum();
    show(4, qty as i32);
    let first = items.iter().map(|i| &i.qty).next();
    show(5, *first.unwrap() as i32);
    let mut longest = "";
    items.iter().map(|i| i.name).for_each(|n| {
        if n.len() > longest.len() {
            longest = n;
        }
    });
    show(6, longest.len() as i32);
    let mut kept: [&u32; 3] = [&0; 3];
    for (slot, q) in kept.iter_mut().zip(items.iter().map(|i| &i.qty)) {
        *slot = q;
    }
    show(7, (*kept[0] * 100 + *kept[1] * 10 + *kept[2]) as i32);
    let mut seen: [&str; 3] = [""; 3];
    let n = seen
        .iter_mut()
        .zip(items.iter().map(|i| i.name))
        .map(|(slot, name)| *slot = name)
        .count();
    show(8, (n * 10 + seen[2].len()) as i32);
    show(9, largest(&items).map_or(0, |i| i.qty) as i32);

    let code = match check("port", Err(7)) {
        Ok(v) => v,
        Err(Error::Bad { key, code }) => key.len() as i32 * 10 + code,
    };
    show(10, code);
    let words = "  alpha beta  ";
    let trimmed = Some(words).map(|s: &str| s.trim()).unwrap_or("");
    show(11, trimmed.len() as i32);
    let bytes: &[u8] = b"xyz";
    let head = Some(bytes).and_then(|b| b.first()).copied().unwrap_or(0);
    show(12, head as i32);
    let pairs = [(1, "one"), (2, "two")];
    let second = pairs.iter().map(|(_, s)| s).nth(1).copied().unwrap_or("");
    show(13, second.len() as i32);
}
