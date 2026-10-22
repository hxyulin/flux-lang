//! Tuple structs' and variants' constructors as function values (`.map(Token::Num)`,
//! `.map(Some)`, `.map(Wrapper)`), generic ones included, passed where
//! closures are and coerced to function pointers.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Num(i64),
    Pair(u8, u8),
    End,
}

#[derive(Clone, Copy)]
struct Wrapper(u32);

struct Gen<T>(T, u8);

struct Named<'a>(&'a str);

fn apply<A, B>(f: impl Fn(A) -> B, a: A) -> B {
    f(a)
}

fn value(t: Token) -> i64 {
    match t {
        Token::Num(n) => n,
        Token::Pair(a, b) => a as i64 * 100 + b as i64,
        Token::End => -1,
    }
}

fn main() {
    let xs = [1i64, 2, 3];
    let total: i64 = xs.iter().copied().map(Token::Num).map(value).sum();
    show(1, total);
    let w: Option<Wrapper> = Some(7).map(Wrapper);
    show(2, w.map_or(0, |w| w.0) as i64);
    let somes: [Option<u8>; 2] = [3u8, 4].map(Some);
    show(3, somes.iter().flatten().map(|&x| x as i64).sum());
    let pair = apply(|(a, b)| Token::Pair(a, b), (1u8, 2u8));
    show(4, value(pair));
    let make: fn(u8, u8) -> Token = Token::Pair;
    show(5, value(make(3, 4)));
    let g = apply(|x| Gen(x, 9), 5i32);
    show(6, g.0 as i64 * 10 + g.1 as i64);
    let f = Gen::<u16>;
    let h = f(40, 2);
    show(7, h.0 as i64 + h.1 as i64);
    let n = apply(Named, "four");
    show(8, n.0.len() as i64);
    let ok: Result<u8, i32> = Ok(5u8).map(Some).map(|o| o.unwrap_or(0));
    show(9, ok.unwrap_or(0) as i64);
    let errs: Result<u8, Option<i32>> = Err(3).map_err(Some);
    show(10, errs.err().flatten().unwrap_or(0) as i64);
    let fns = [Token::Num, |n| Token::Num(n * 2)];
    show(11, fns.iter().map(|f| value((*f)(5))).sum());
    show(12, (Token::End == Token::End) as i64);
}
