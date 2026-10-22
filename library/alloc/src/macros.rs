//! `vec!`, which every crate sees, as rustc's `std` gives it.

/// The text the format string and its arguments make, as a `String`.
#[macro_export]
macro_rules! format {
    ($($arg:tt)*) => {
        $crate::fmt::format(format_args!($($arg)*))
    };
}

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
