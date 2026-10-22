//! `vec!`, which every crate sees, as rustc's `std` gives it.

/// A `Vec` of the elements written, or of `n` clones of one.
#[macro_export]
macro_rules! vec {
    () => {
        $crate::vec::Vec::new()
    };
    ($elem:expr; $n:expr) => {
        $crate::vec::from_elem($elem, $n)
    };
    ($($x:expr),+ $(,)?) => {
        $crate::vec::Vec::__from_array([$($x),+])
    };
}
