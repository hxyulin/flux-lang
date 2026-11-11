fn apply<F: FnOnce(&(u8, u16)) -> u16>(f: F) -> u16 {
    f(&(1, 2))
}
fn shared<F: Fn(&(u8, u16)) -> u16>(f: F) -> u16 {
    f(&(3, 4))
}
fn mutable<F: FnMut(&(u8, u16)) -> u16>(mut f: F) -> u16 {
    f(&(5, 6))
}
struct Callback<F: Fn(&(u8, u16)) -> u16> {
    callback: F,
}
fn main() {
    let direct = apply(|(_, b)| *b);
    let captured = apply({
        let extra = 3;
        move |(_, b)| *b + extra
    });
    let nested = apply({
        let extra = 4;
        {
            let twice = extra * 2;
            move |(_, b)| *b + twice
        }
    });
    let borrowed = shared(&{
        let extra = 5;
        move |(_, b): &(u8, u16)| *b + extra
    });
    let mut count = 0;
    let unique = mutable(&mut {
        |(_, b): &(u8, u16)| {
            count += 1;
            *b + count
        }
    });
    let boxed: Box<dyn Fn(u16) -> u16> = Box::new({
        let extra = 7;
        move |x| x + extra
    });
    let holder = Callback {
        callback: { move |(_, value)| value.count_ones() as u16 },
    };
    assert_eq!((holder.callback)(&(0, 7)), 3);
    assert_eq!(
        (direct, captured, nested, borrowed, unique, boxed(8)),
        (2, 5, 10, 9, 7, 15)
    );
    println!(
        "{} {} {} {} {} {}",
        direct,
        captured,
        nested,
        borrowed,
        unique,
        boxed(8)
    );
}
