//! A `vis` fragment that matched nothing, forwarded to another macro,
//! still matches a `$v:vis` there, even right before a closing delimiter
//! or at the end of the input. `$($t:tt)*` keeps it, and an `item`
//! fragment reads through it.

macro_rules! at_close {
    ([$a:vis $kind:ident $name:ident $b:vis]) => {
        again!([$b])
    };
}

macro_rules! again {
    ([$v:vis]) => {
        at_end!($v)
    };
}

macro_rules! at_end {
    ($v:vis) => {
        1
    };
}

macro_rules! through_tts {
    ([$($t:tt)*]) => {
        again!([$($t)*])
    };
}

macro_rules! listed {
    ([$a:vis , $b:vis]) => {
        2
    };
}

macro_rules! item {
    ($i:item) => {
        $i
    };
}

macro_rules! outer {
    ($v:vis $kind:ident $name:ident) => {
        item!($v fn $name() -> i32 { 4 });
        fn main() {
            let total = at_close!([$v $kind $name $v]) + through_tts!([$v]) + listed!([$v , pub]);
            println!("{total} {}", $name());
            std::process::exit(total + $name());
        }
    };
}

outer!(struct four);
