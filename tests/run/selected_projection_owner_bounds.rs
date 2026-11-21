struct Bytes<'a, const N: u128>(&'a [u8]);

trait Contiguous<'a> {
    const CONTIGUOUS: bool;
}

impl<'a, const N: u128> Contiguous<'a> for Bytes<'a, N> {
    const CONTIGUOUS: bool = N != 0;
}

struct Digits<'a: 'b, 'b, const N: u128> {
    input: &'a [u8],
    index: &'b mut usize,
}

impl<'a: 'b, 'b, const N: u128> Contiguous<'a> for Digits<'a, 'b, N> {
    const CONTIGUOUS: bool = Bytes::<'a, N>::CONTIGUOUS;
}

impl<'a: 'b, 'b, const N: u128> Iterator for Digits<'a, 'b, N> {
    type Item = &'a u8;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.input.get(*self.index)?;
        *self.index += 1;
        Some(value)
    }
}

trait Peek<'a>: Iterator<Item = &'a u8> {
    fn peek(&mut self) -> Option<Self::Item>;
}

impl<'a: 'b, 'b, const N: u128> Peek<'a> for Digits<'a, 'b, N> {
    fn peek(&mut self) -> Option<<Self as Iterator>::Item> {
        self.input.get(*self.index)
    }
}

fn main() {
    let input = vec![41, 42];
    let mut index = 0;
    let value = {
        let mut digits = Digits::<5> { input: &input, index: &mut index };
        assert_eq!(digits.next(), Some(&41));
        digits.peek().unwrap()
    };
    println!("{} {} {}", value, Digits::<5>::CONTIGUOUS, Digits::<0>::CONTIGUOUS);
}
