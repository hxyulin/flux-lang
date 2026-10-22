mod operations {
    pub trait Measure {
        fn measure(&self) -> u32;
    }
    impl Measure for u32 {
        fn measure(&self) -> u32 { *self }
    }
}
struct S;
impl operations::Measure for S {
    fn measure(&self) -> u32 { 3u32.measure() }
}
// The qualified impl header does not import the trait into the module.
fn outside() -> u32 { 3u32.measure() }
fn no_clone(elements: Box<[S]>) { let _ = elements.clone(); }
