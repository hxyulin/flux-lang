use std::{collections::BTreeSet, fmt, time::Duration};

#[derive(Debug)]
struct Code(u8);

impl Code {
    const FIRST: Self = Self::new(3);
    const SECOND: Self = Self::new(4);

    const fn new(n: u8) -> Self {
        Self(n + 250)
    }

    const fn all() -> &'static [Self] {
        &[Self::FIRST, Self::SECOND]
    }
}

fn main() {
    const CODES: &[Code] = Code::all();
    assert_eq!((CODES[0].0, CODES[1].0), (253, 254));
    assert_eq!(Code::all()[1].0, 254);
    let formatted = fmt::from_fn(|f| write!(f, "code {}", CODES[0].0));
    assert_eq!(format!("{formatted} / {formatted:?}"), "code 253 / code 253");

    let times = [Duration::new(0, 950_000_000), Duration::from_millis(100)];
    let owned: Duration = times.into_iter().sum();
    let borrowed: Duration = times.iter().sum();
    assert_eq!(owned, Duration::new(1, 50_000_000));
    assert_eq!(borrowed, owned);

    let left = BTreeSet::from([1, 3, 5, 7]);
    let right = BTreeSet::from([0, 1, 4, 5, 9]);
    assert_eq!(left.difference(&right).copied().collect::<Vec<_>>(), [3, 7]);

    let empty = std::slice::Iter::<Code>::default();
    assert_eq!(empty.len(), 0);
    let suffix = b"symbol".as_slice();
    assert_eq!([b".weak.", suffix].concat().as_slice(), b".weak.symbol");
    assert_eq!([suffix, b".weak."].concat().as_slice(), b"symbol.weak.");
    assert_eq!([b"<", suffix, b">>"].concat().as_slice(), b"<symbol>>");
    println!("{formatted}");
}
