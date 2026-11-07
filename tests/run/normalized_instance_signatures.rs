trait Finish {
    type Output;
    fn finish(self) -> Self::Output;
}

impl Finish for u16 {
    type Output = (u8, u64);
    fn finish(self) -> Self::Output {
        (self as u8, self as u64 + 100)
    }
}

impl Finish for u32 {
    type Output = [u32; 3];
    fn finish(self) -> Self::Output {
        [self, self + 1, self + 2]
    }
}

#[track_caller]
#[inline(never)]
fn convert<T: Finish>(value: T) -> T::Output {
    value.finish()
}

#[inline(never)]
fn first() -> u64 {
    let pair = convert(7u16);
    pair.0 as u64 + pair.1
}

#[inline(never)]
fn second() -> u32 {
    convert(7u32)[1]
}

fn main() {
    let pointer: fn(u16) -> (u8, u64) = convert::<u16>;
    println!("{} {} {}", first(), second(), pointer(9).1);
}
