//! Standard-library macros with ordinary Rust item expansion.

#[macro_export]
macro_rules! __is_aarch64_feature_detected {
    ("crc" $(,)?) => {{
        #[cfg(target_arch = "aarch64")]
        {
            $crate::arch::__crc_detected()
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            compile_error!("aarch64 feature detection requires an aarch64 target")
        }
    }};
    ("dit" $(,)?) => {{
        #[cfg(target_arch = "aarch64")]
        { $crate::arch::__aarch64_dit_detected() }
        #[cfg(not(target_arch = "aarch64"))]
        { compile_error!("aarch64 feature detection requires an aarch64 target") }
    }};
    ("sb" $(,)?) => {{
        #[cfg(target_arch = "aarch64")]
        { $crate::arch::__aarch64_sb_detected() }
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
    ($(#[$attr:meta])* $vis:vis static $name:ident: $ty:ty = const { $($init:tt)* }) => {
        $crate::thread_local! { $(#[$attr])* $vis static $name: $ty = const { $($init)* }; }
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: $ty:ty = $init:expr) => {
        $crate::thread_local! { $(#[$attr])* $vis static $name: $ty = $init; }
    };
}

/// Detects a supported x86 SIMD feature, including OS support for AVX state.
#[macro_export]
macro_rules! is_x86_feature_detected {
    ("sse" $(,)?) => {
        $crate::arch::__x86_detected("sse")
    };
    ("sse2" $(,)?) => {
        $crate::arch::__x86_detected("sse2")
    };
    ("sse3" $(,)?) => {
        $crate::arch::__x86_detected("sse3")
    };
    ("ssse3" $(,)?) => {
        $crate::arch::__x86_detected("ssse3")
    };
    ("sse4.1" $(,)?) => {
        $crate::arch::__x86_detected("sse4.1")
    };
    ("sse4.2" $(,)?) => {
        $crate::arch::__x86_detected("sse4.2")
    };
    ("avx" $(,)?) => {
        $crate::arch::__x86_detected("avx")
    };
    ("avx2" $(,)?) => {
        $crate::arch::__x86_detected("avx2")
    };
    ("avx512f" $(,)?) => {
        $crate::arch::__x86_detected("avx512f")
    };
    ("pclmulqdq" $(,)?) => {
        $crate::arch::__x86_detected("pclmulqdq")
    };
    ("vpclmulqdq" $(,)?) => {
        $crate::arch::__x86_detected("vpclmulqdq")
    };
    ("sha" $(,)?) => {
        $crate::arch::__x86_detected("sha")
    };
    ("xsave" $(,)?) => {
        $crate::arch::__x86_detected("xsave")
    };
    ($($tokens:tt)*) => {
        compile_error!("unsupported x86 feature name")
    };
}
