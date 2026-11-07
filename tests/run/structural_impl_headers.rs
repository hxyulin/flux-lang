struct Pair<T>(T, T);

trait Pick<T> {
    fn pick(&self, value: T) -> u32;
}

impl<T> Pick<T> for Pair<T> {
    fn pick(&self, _value: T) -> u32 {
        11
    }
}

impl Pick<char> for Pair<bool> {
    fn pick(&self, _value: char) -> u32 {
        23
    }
}

trait Output {
    type Value;
}

struct Source;
impl Output for Source {
    type Value = i32;
}

struct Wrapped<T>(T);
impl<T: Output> Pick<T::Value> for Wrapped<T> {
    fn pick(&self, _value: T::Value) -> u32 {
        37
    }
}

impl<T: ?Sized> Pick<()> for &T {
    fn pick(&self, _value: ()) -> u32 {
        41
    }
}

fn choose<P: Pick<T>, T>(pair: &P, value: T) -> u32 {
    pair.pick(value)
}

fn main() {
    let pair = Pair(true, false);
    let mut total = 0;
    for _ in 0..4 {
        total += choose(&pair, true);
        total += choose(&pair, 'x');
        total += choose(&Pair(1i32, 2i32), 3i32);
        total += choose(&Wrapped(Source), 7i32);
        total += choose(&"unsized", ());
    }
    println!("{}", total);
    assert_eq!(total, 492);
}
