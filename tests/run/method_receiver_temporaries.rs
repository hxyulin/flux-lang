//! Borrowing a method result mutably must not make its input receiver's
//! overloaded dereferences mutable. The impl's qualified trait is in scope
//! only inside that impl, including for other receivers of the same trait.
mod operations {
    pub trait Measure {
        fn measure(&self) -> usize;
    }
    impl Measure for u32 {
        fn measure(&self) -> usize {
            *self as usize
        }
    }
}
struct Data(Vec<u32>);
impl operations::Measure for Data {
    fn measure(&self) -> usize {
        let count = self.0.windows(2).filter(|w| w[0] < w[1]).count();
        count + 3u32.measure()
    }
}
fn ordered(v: &Vec<u32>) -> bool {
    !v.windows(2).any(|w| w[0] >= w[1])
}
fn main() {
    let data = Data(vec![1, 3, 5]);
    println!(
        "{} {}",
        operations::Measure::measure(&data),
        ordered(&data.0)
    );
    println!("{}", ordered(&vec![3, 1]));
}
