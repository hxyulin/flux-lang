macro_rules! choose {
    ($($item:item)*) => { $($item)* };
}
choose! {
    #[cfg(any())]
    #[path = "missing.rs"]
    mod disabled;
    #[cfg_attr(all(), path = "support/macro_path/outer.rs")]
    mod chosen;
}
fn main() { println!("{}", chosen::value()); }
