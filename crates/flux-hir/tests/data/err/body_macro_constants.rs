macro_rules! typed {
    ($n:expr) => {
        let _: [u8; $n] = [0; 4];
    };
}
fn generic<const N: usize>() {
    typed!(N + 1);
}
fn main() {
    typed!(2 + 2);
    generic::<3>();
}
