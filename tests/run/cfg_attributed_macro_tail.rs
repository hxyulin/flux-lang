macro_rules! pick {
    () => {
        pick!(@cfg ; if true { return nonexistent(); });
        #[cfg(all())]
        pick!(@cfg any(); if true { return nonexistent(); })
    };
    (@cfg ; $ex:expr) => {};
    (@cfg $provided:meta; $ex:expr) => { #[cfg($provided)] $ex };
}
fn select() -> i32 { pick!(); 7 }
fn main() { println!("{}", select()); }
