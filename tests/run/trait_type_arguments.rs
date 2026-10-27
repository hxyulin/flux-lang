trait Read<T> {
    fn read(self, value: T) -> T;

    fn again(self, value: T) -> T
    where
        Self: Sized,
    {
        self.read(value)
    }
}

impl<T> Read<T> for () {
    fn read(self, value: T) -> T {
        value
    }
}

struct Wrap<R>(R);

impl<'a, R: Read<&'a u32>> Read<&'a u32> for Wrap<R> {
    fn read(self, value: &'a u32) -> &'a u32 {
        self.0.read(value)
    }
}

fn shared<'a, R: Read<&'a u32>>(reader: R, value: &'a u32) -> &'a u32 {
    reader.again(value)
}

fn exclusive<'a, R: Read<&'a mut [u32]>>(reader: R, value: &'a mut [u32]) -> &'a mut [u32] {
    reader.read(value)
}

trait Pick<A, B> {
    fn pick(self, first: A, second: B) -> (A, B);
}

impl<A, B> Pick<A, B> for () {
    fn pick(self, first: A, second: B) -> (A, B) {
        (first, second)
    }
}

fn pair<'a, 'b, R: Pick<&'a u32, &'b u32>>(
    reader: R,
    first: &'a u32,
    second: &'b u32,
) -> (&'a u32, &'b u32) {
    reader.pick(first, second)
}

struct Borrowed<'a>(&'a str);

impl<'a> From<&'a str> for Borrowed<'a> {
    fn from(value: &'a str) -> Self {
        Self(value)
    }
}

fn convert<'a, Q: ?Sized, T: From<&'a Q>>(value: &'a Q) -> T {
    T::from(value)
}

fn main() {
    let number = 17;
    let result = shared(Wrap(()), &number);
    assert!(core::ptr::eq(result, &number));
    let (first, second) = pair((), &number, &29);
    assert_eq!((*first, *second), (17, 29));

    let mut values = [1, 2, 3];
    let view = exclusive((), &mut values);
    view[0] += 6;
    view[2] *= 3;
    assert_eq!(values, [7, 2, 9]);

    let text = String::from("borrowed café");
    let borrowed: Borrowed<'_> = convert(text.as_str());
    let owned: String = convert(text.as_str());
    assert!(core::ptr::eq(borrowed.0.as_ptr(), text.as_ptr()));
    assert_eq!(borrowed.0, owned.as_str());
    assert_ne!(owned.as_ptr(), text.as_ptr());
    println!(
        "trait arguments {} {} {} {}",
        result, second, values[2], owned
    );
}
