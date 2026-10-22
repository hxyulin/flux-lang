//! Exercise pooled expression/pattern lists and shared place projections
//! across control-flow joins, partial moves, closures, and generic bodies.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Token(i32);

impl Drop for Token {
    fn drop(&mut self) {
        unsafe {
            printf(c"drop %d\n".as_ptr(), self.0);
        }
    }
}

struct Packet {
    pair: (Token, Token),
    data: [i32; 4],
}

enum Choice {
    Packet(Packet),
    Empty,
}

fn identity<T>(value: T) -> T {
    value
}

fn evaluate(mut count: i32) -> i32 {
    let mut packet = Packet {
        pair: (Token(3), Token(7)),
        data: [2, 4, 6, 8],
    };
    while count > 0 {
        let selected = if count % 2 == 0 {
            &mut packet.data[1]
        } else {
            &mut packet.data[2]
        };
        *selected += count;
        count -= 1;
    }
    let choice = identity(Choice::Packet(packet));
    let result = match choice {
        Choice::Packet(Packet {
            pair: (first, second),
            data: [a, b, c, d],
        }) => {
            let sum = |offset: i32| first.0 + second.0 + a + b + c + d + offset;
            let value = sum(1) + sum(2);
            drop(first);
            value
        }
        Choice::Empty => 0,
    };
    let empty = Choice::Empty;
    match empty {
        Choice::Empty => result,
        Choice::Packet(_) => 0,
    }
}

fn main() {
    unsafe {
        printf(c"result %d\n".as_ptr(), evaluate(5));
    }
}
