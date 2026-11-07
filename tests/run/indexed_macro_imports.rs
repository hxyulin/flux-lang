use std as runtime;
use runtime::println as emit;

macro_rules! declare {
    () => {
        use runtime::vec as make;
        fn values() -> runtime::vec::Vec<u32> { make![3, 5, 7] }
    };
}
declare!();

fn main() {
    emit!("{}", values().iter().sum::<u32>());
    {
        use runtime::println as nested;
        macro_rules! report { () => { nested!("{}", values().len()) }; }
        { report!(); }
    }
}
