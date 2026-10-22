//! `Box`: values on the heap, dereferenced, mutated, compared, cloned,
//! printed, recursive data (`Box<List>`), boxes of values without size,
//! `into_raw`/`from_raw` and `leak`, and drops: the value when its box
//! is dropped, in rustc's order.

#[derive(Debug)]
enum List {
    Cons(i32, Box<List>),
    Nil,
}

use List::{Cons, Nil};

fn sum(list: &List) -> i32 {
    match list {
        Cons(v, rest) => v + sum(rest),
        Nil => 0,
    }
}

struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn make(name: &'static str) -> Box<Noisy> {
    Box::new(Noisy(name))
}

fn main() {
    let mut b = Box::new(5);
    *b += 1;
    println!("{b} {:?} {}", b, *b * 2);
    let list = Cons(1, Box::new(Cons(2, Box::new(Cons(3, Box::new(Nil))))));
    println!("{} {list:?}", sum(&list));
    let pair = Box::new((1u8, String::from("pair")));
    println!("{} {}", pair.0, pair.1.len());
    let c = pair.clone();
    println!("{} {}", c == pair, Box::new(3) < Box::new(4));
    let unit = Box::new(());
    println!("{:?}", unit);
    let raw = Box::into_raw(Box::new(41));
    let back = unsafe { Box::from_raw(raw) };
    println!("{}", *back + 1);
    let leaked: &mut i32 = Box::leak(Box::new(7));
    *leaked += 1;
    println!("{leaked}");
    let a = make("a");
    let mut v = vec![make("v1"), make("v2")];
    {
        let _inner = make("inner");
        println!("scope");
    }
    v.remove(0);
    println!("{} {}", a.0, v[0].0);
    let nested = Box::new(Box::new(Noisy("nested")));
    println!("{}", nested.0);
    println!("end");
}
