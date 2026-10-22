//! Nested child lists share lowering scratch; siblings must keep source order.
unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Pair {
    first: i32,
    second: i32,
}

impl Pair {
    fn combine(&self, x: i32, y: i32) -> i32 {
        self.first + self.second + x * 10 + y
    }
}

fn combine(x: i32, y: i32) -> i32 {
    x * 10 + y
}

fn calculate(seed: i32) -> i32 {
    macro_rules! add {
        ($value:expr, $target:ident) => {
            $target = $target + $value;
        };
    }
    let pair = Pair {
        first: combine(seed, combine(2, 3)),
        second: Pair {
            first: 4,
            second: 5,
        }
        .combine(6, 7),
    };
    let tuple = (combine(1, 2), (combine(3, 4), combine(5, 6)), 7);
    let arrays = [[combine(1, 2), 3], [4, combine(5, 6)]];
    let mut result = combine(combine(1, 2), combine(3, 4));
    add!(
        {
            let mut inner = 10;
            add!(combine(2, 3), inner);
            inner
        },
        result
    );
    let nested = |x: i32, y: i32| {
        let inner = |a: i32, b: i32| combine(a, b);
        combine(x, inner(y, 2))
    };
    result = result + nested(3, 4);
    result = result
        + match (seed, tuple) {
            (0 | 1, (a, (b, c), d)) => a + b + c + d,
            (x, (a, (b, c), d)) if x > 1 => match arrays {
                [[first, ..], [.., last]] => x + a + b + c + d + first + last,
            },
            _ => 0,
        };
    let Pair { first, second } = pair;
    result + first + second
}

fn main() {
    unsafe {
        printf(c"%d %d\n".as_ptr(), calculate(0), calculate(3));
    }
}
