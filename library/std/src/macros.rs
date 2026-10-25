//! Standard-library macros with ordinary Rust item expansion.

#[macro_export]
macro_rules! thread_local {
    () => {};
    ($(#[$attr:meta])* $vis:vis static $name:ident: $ty:ty = const { $($init:tt)* }; $($rest:tt)*) => {
        $(#[$attr])*
        $vis static $name: $crate::thread::LocalKey<$ty> =
            $crate::thread::LocalKey::__new(|| const { $($init)* });
        $crate::thread_local! { $($rest)* }
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: $ty:ty = $init:expr; $($rest:tt)*) => {
        $(#[$attr])*
        $vis static $name: $crate::thread::LocalKey<$ty> =
            $crate::thread::LocalKey::__new(|| $init);
        $crate::thread_local! { $($rest)* }
    };
}
