// Macro calls expand where they are written. Locals a macro's
// definition names are its own (`t`), or those in scope where it is
// defined (`base`), never the caller's.

macro_rules! swap {
    ($a:ident, $b:ident) => {
        let t = $a;
        $a = $b;
        $b = t;
    };
}

macro_rules! first {
    ($x:expr $(, $rest:expr)*) => {
        $x
    };
}

macro_rules! pair_ty {
    ($t:ty) => {
        ($t, $t)
    };
}

macro_rules! some {
    ($p:pat) => {
        Some($p)
    };
}

macro_rules! getter {
    ($name:ident: $t:ty = $v:expr) => {
        fn $name() -> $t {
            $v
        }
    };
}

getter!(answer: i32 = 42);

fn hygiene(t: i32) -> i32 {
    let mut a = t;
    let mut b = 2;
    swap!(a, b);
    let base = 10;
    macro_rules! plus_base {
        ($e:expr) => {
            $e + base
        };
    }
    let base = 1000;
    plus_base!(a) + b + base
}

fn main() {
    let p: pair_ty!(u8) = (1, 2);
    let o = Some(p.0);
    let n = match o {
        some!(x) => x,
        None => 0,
    };
    let f = first!(n, 2, 3);
    let _ = hygiene(f as i32) + answer();
    let _ = matches!(o, Some(1..=3));
}
