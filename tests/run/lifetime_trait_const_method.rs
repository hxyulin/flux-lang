struct Bytes<'a, const N: u128>(&'a [u8]);

impl<const N: u128> Bytes<'_, N> {
    fn tag(&self) -> u128 {
        N
    }
}

trait AsBytes<'a> {
    fn bytes<const N: u128>(&'a self) -> Bytes<'a, N>;
}

impl<'a> AsBytes<'a> for [u8] {
    fn bytes<const N: u128>(&'a self) -> Bytes<'a, N> {
        Bytes(self)
    }
}

fn main() {
    let owned = vec![23, 19];
    let bytes = owned.as_slice().bytes::<7>();
    println!("{} {}", bytes.0[0] + bytes.0[1], bytes.tag());
    println!("{}", owned.as_slice().bytes::<11>().tag());
}
