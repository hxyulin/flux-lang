trait Read {
    fn read(self) -> u8;
}
struct S(u8);
impl Read for &S {
    fn read(self) -> u8 {
        self.0
    }
}
fn read(s: S) -> u8 {
    s.read()
}
fn convert(s: &[u8]) -> [u8; 4] {
    s[..4].try_into().unwrap()
}
fn main() {}
