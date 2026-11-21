trait Convert<T, U = T, V = Self> {
    fn combine(self, value: T, other: U) -> V;
}

impl Convert<u32> for u8 {
    fn combine(self, value: u32, other: u32) -> u8 {
        self + value as u8 + other as u8
    }
}

fn main() {
    assert_eq!(<u8 as Convert<u32>>::combine(1, 2, 3), 6);
    assert_eq!(Convert::<u32>::combine(4u8, 5, 6), 15);
    let combine = <u8 as Convert<u32, u32>>::combine;
    assert_eq!(combine(7, 8, 9), 24);
}
