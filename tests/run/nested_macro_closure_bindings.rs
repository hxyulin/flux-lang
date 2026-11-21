macro_rules! flatten {
    (@tuple $pattern:pat => ($($out:tt)*)) => {
        |$pattern| ($($out)*)
    };
    (@tuple $pattern:pat => ($($out:tt)*), $next:expr $(, $rest:expr)*) => {
        flatten!(@tuple ($pattern, b) => ($($out)*, b) $(, $rest)*)
    };
    ($first:expr, $($tail:expr),+) => {
        flatten!(@tuple a => (a), $($tail),+)
    };
}

fn main() {
    let flatten = flatten!(1, 2, 3);
    assert_eq!(flatten(((11, 22), 33)), (11, 22, 33));
    println!("{:?}", flatten(((4, 5), 6)));
}
