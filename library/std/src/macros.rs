//! Standard-library macros with ordinary Rust item expansion.

#[macro_export]
macro_rules! __is_aarch64_feature_detected {
    ("crc" $(,)?) => {{
        #[cfg(target_arch = "aarch64")]
        { $crate::arch::__crc_detected() }
        #[cfg(not(target_arch = "aarch64"))]
        { compile_error!("aarch64 feature detection requires an aarch64 target") }
    }};
    ($feature:literal $(,)?) => {
        compile_error!("aarch64 feature detection for this feature is not supported yet")
    };
    ($($tokens:tt)*) => {
        compile_error!("expected an aarch64 feature name string literal")
    };
}

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
