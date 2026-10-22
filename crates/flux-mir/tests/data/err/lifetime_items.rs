struct Words<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for Words<'a> {
    type Item = &'a u8;
    fn next(&mut self) -> Option<&'a u8> {
        if self.pos < self.src.len() {
            self.pos += 1;
            Some(&self.src[self.pos - 1])
        } else {
            None
        }
    }
}

enum Token<'a> {
    Num(&'a i32),
    End,
}

fn store<'a, T>(slot: &mut Option<&'a T>, v: &'a T) {
    *slot = Some(v);
}

fn user_iter_escapes() -> Option<&'static u8> {
    let bytes = [1u8, 2];
    let mut w = Words { src: &bytes, pos: 0 };
    w.next()
}

fn token_escapes() {
    let t;
    {
        let x = 5;
        t = Token::Num(&x);
    }
    match t {
        Token::Num(n) => {
            let _ = *n;
        }
        Token::End => {}
    }
}

fn store_short() {
    let a = 1;
    let mut slot = Some(&a);
    {
        let b = 2;
        store(&mut slot, &b);
    }
    let _ = slot;
}

fn shorter_self<'a>(w: &Words<'a>) -> &'a [u8] {
    w.src
}

fn bad_self<'a>(w: &'a Words<'_>) -> &'static [u8] {
    w.src
}

fn main() {}
