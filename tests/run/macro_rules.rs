//! `macro_rules!` macros in every position: expressions, statements,
//! items, patterns and types, with fragments, nested repetitions,
//! `#[macro_use]` and `#[macro_export]`, and hygiene (`tmp`).

unsafe extern "C" {
    fn printf(fmt: *const i8, ...) -> i32;
}

#[macro_use]
mod helpers {
    macro_rules! twice {
        ($e:expr) => {
            2 * $e
        };
    }
}

mod exported {
    #[macro_export]
    macro_rules! triple {
        ($e:expr) => {
            3 * $e
        };
    }
}

macro_rules! point {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Clone, Copy)]
        struct $name {
            $($field: $ty),*
        }
        impl $name {
            fn sum(&self) -> i64 {
                0 $(+ self.$field as i64)*
            }
        }
    };
}

point!(P { x: i32, y: u8, z: i64 });

macro_rules! swap_let {
    ($a:ident, $b:ident) => {
        let tmp = $a;
        $a = $b;
        $b = tmp;
    };
}

macro_rules! nested {
    ($($outer:ident => [$($inner:expr),*]);* $(;)?) => {
        0 $($(+ $inner)* + $outer)*
    };
}

macro_rules! is_small {
    ($p:pat) => {
        match 3u8 {
            $p => 1,
            _ => 0,
        }
    };
}

macro_rules! ty_of {
    () => {
        u16
    };
}

macro_rules! zero_pat {
    () => {
        0
    };
}

macro_rules! lit {
    ($l:literal) => {
        $l
    };
}

macro_rules! tts {
    ($($t:tt)*) => {
        $($t)*
    };
}

macro_rules! blk {
    ($b:block) => {
        $b
    };
}

macro_rules! labeled {
    ($body:expr) => {
        'outer: loop {
            loop {
                break 'outer $body;
            }
        }
    };
}

fn hygiene() -> i32 {
    let tmp = 40;
    let mut a = 1;
    let mut b = 2;
    swap_let!(a, b);
    tmp + a * 10 + b
}

fn main() {
    let p = P { x: 1, y: 2, z: 3 };
    let q = p;
    let one = 1;
    let x: ty_of!() = 7;
    let z = match 0 {
        zero_pat!() => 5,
        _ => 6,
    };
    let n = nested!(one => [2, 3]; one => [4]);
    let t = tts!(1 + 2 * 3);
    let b = blk!({ let k = 4; k * k });
    let l = labeled!(9);
    let r = crate::triple!(2) + triple!(1);
    unsafe {
        printf(
            c"%d %lld %d %d %d %d %d %d %d %d %d %d\n".as_ptr(),
            twice!(5),
            q.sum(),
            hygiene(),
            is_small!(0..=5),
            x as i32,
            z,
            n,
            t,
            b,
            l,
            r,
            lit!(-4),
        );
    }
}
