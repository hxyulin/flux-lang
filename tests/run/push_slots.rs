use core::sync::atomic::{AtomicUsize, Ordering};

static DROPS: AtomicUsize = AtomicUsize::new(0);

struct Owner(u64);

impl Drop for Owner {
    fn drop(&mut self) {
        DROPS.fetch_add(self.0 as usize, Ordering::Relaxed);
    }
}

enum Token {
    Small(Owner, u64),
    Large([u64; 24]),
    Fields(Fields),
}

struct Fields {
    owner: Owner,
    values: [u64; 5],
}

#[inline(never)]
fn fields(x: u64) -> Fields {
    Fields {
        owner: Owner(x),
        values: [2 * x; 5],
    }
}

#[inline(never)]
fn push_nested(stack: &mut Vec<(Token, usize, usize)>, x: u64) {
    let later = fields(x);
    stack.push((Token::Fields(later), x as usize, 3 * x as usize));
}

#[inline(never)]
fn push_constructed(stack: &mut Vec<(Token, usize, usize)>, x: u64) {
    stack.push((Token::Small(Owner(x), 2 * x), x as usize, 3 * x as usize));
}

#[inline(never)]
fn make_small(value: u64) -> Token {
    Token::Small(Owner(value), 2 * value)
}

#[inline(never)]
fn push_result(stack: &mut Vec<(Token, usize, usize)>, x: u64) {
    let token = make_small(x);
    stack.push((token, x as usize, 3 * x as usize));
}

fn main() {
    let mut stack = Vec::with_capacity(1);
    push_constructed(&mut stack, 3);
    push_constructed(&mut stack, 5);
    push_result(&mut stack, 7);
    stack.push((Token::Large([23; 24]), 11, 13));
    push_nested(&mut stack, 11);
    push_nested(&mut stack, 13);
    for (token, start, end) in &stack {
        match token {
            Token::Small(owner, value) => println!("{} {} {start} {end}", owner.0, value),
            Token::Fields(fields) => {
                println!("{} {} {start} {end}", fields.owner.0, fields.values[4])
            }
            Token::Large(payload) => println!("{} {} {start} {end}", payload[0], payload[23]),
        }
    }
    drop(stack);
    println!("{}", DROPS.load(Ordering::Relaxed));
}
