// Debug assertions follow rustc: on without `-O`, off with it.

macro_rules! local_check {
    () => {
        cfg!(debug_assertions)
    };
}

#[cfg(debug_assertions)]
fn mode() -> &'static str {
    "debug assertions"
}

#[cfg(not(debug_assertions))]
fn mode() -> &'static str {
    "no debug assertions"
}

fn main() {
    println!("{} {} {}", mode(), cfg!(debug_assertions), local_check!());
    let mut checked = 0;
    debug_assert!({
        checked += 1;
        true
    });
    debug_assert_eq!(
        {
            checked += 1;
            2
        },
        2
    );
    debug_assert_ne!(checked, 7);
    println!("debug assertions evaluated {checked} times");
}
