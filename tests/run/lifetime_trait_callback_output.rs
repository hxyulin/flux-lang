trait Invoke {
    type Result;
}

impl<R> Invoke for dyn Fn() -> R + '_ {
    type Result = R;
}

unsafe trait Family<'a, R> {
    type Output: ?Sized + Invoke<Result = R>;
    fn callback() -> unsafe extern "C-unwind" fn() -> u32;
}

unsafe extern "C-unwind" fn answer() -> u32 {
    42
}

// SAFETY: the callback has the declared ABI and its return type is fixed.
unsafe impl<'a, R, F: Fn() -> R + 'a> Family<'a, R> for F {
    type Output = dyn Fn() -> R + 'a;

    fn callback() -> unsafe extern "C-unwind" fn() -> u32 {
        answer
    }
}

fn main() {
    let callback = <fn() -> u32 as Family<'_, u32>>::callback();
    // SAFETY: the callback's implementation takes no arguments and returns a constant.
    assert_eq!(unsafe { callback() }, 42);
}
