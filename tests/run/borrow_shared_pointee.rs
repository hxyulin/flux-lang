//! Borrowing through a shared reference protects its pointee, while the
//! struct storing the reference may still be borrowed and changed.

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl Lexer<'_> {
    fn error(&self) -> usize {
        self.pos
    }

    fn hex(&mut self, i: usize) -> Result<usize, usize> {
        let bytes = self.src.as_bytes();
        let hex = bytes.get(i..i + 4).ok_or_else(|| {
            self.pos = i;
            self.error()
        })?;
        Ok(hex.len())
    }
}

fn iterator_output(src: &str) -> Option<&str> {
    let mut tokens = [Ok::<_, ()>(src), Ok(src), Ok(src)]
        .into_iter()
        .map_while(Result::ok)
        .map(|t| t)
        .peekable();
    for _ in 0..2 {
        tokens.next()?;
    }
    tokens.next()
}

fn replace<'a>(s: &mut &'a str, new: &'a str) {
    *s = new;
}

fn main() {
    let mut lexer = Lexer { src: "abcd", pos: 0 };
    println!("{:?} {:?} {}", lexer.hex(0), lexer.hex(1), lexer.pos);
    println!("{:?}", iterator_output("borrowed"));
    println!("{}", static_output());
    let f: fn(&str) -> Option<&str> = keep;
    let mut iter = ["pointer"].into_iter().map_while(f);
    println!("{:?}", iter.next());
    let mut source = "old";
    let old = &*source;
    replace(&mut source, "new");
    println!("{} {}", old, source);

    // A mutable raw pointer permits writes even through a shared
    // reference to the pointer, without making that reference mutable.
    let mut n = 1;
    let p: *mut i32 = &mut n;
    let r = &p;
    unsafe { **r += 2 };
    println!("{}", n);
}

fn keep(x: &str) -> Option<&str> {
    Some(x)
}

fn constant(_: &str) -> Option<&'static str> {
    Some("static")
}

fn static_output() -> &'static str {
    let bytes = [b'a'];
    let text = core::str::from_utf8(&bytes).unwrap();
    [text].into_iter().map_while(constant).next().unwrap()
}
