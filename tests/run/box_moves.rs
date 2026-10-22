//! Moving out of a `Box`, as rustc's built-in box dereference allows:
//! all of `*b`, a field of it (the rest dropped with the box), on some
//! paths only (with drop flags), through a function, and `*b` written
//! and borrowed where `b` is mutable. The box's memory is freed once
//! either way.

struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

struct Pair {
    a: Noisy,
    b: Noisy,
}

fn take(b: Box<Noisy>) -> Noisy {
    *b
}

fn pick(flag: bool) -> Noisy {
    let b = Box::new(Pair {
        a: Noisy("left"),
        b: Noisy("right"),
    });
    if flag { b.a } else { b.b }
}

fn main() {
    let b = Box::new(Noisy("one"));
    let n = *b;
    println!("moved {}", n.0);
    let p = Box::new(Pair {
        a: Noisy("pa"),
        b: Noisy("pb"),
    });
    let a = p.a;
    println!("took {}", a.0);
    let t = take(Box::new(Noisy("t")));
    println!("got {}", t.0);
    println!("picked {}", pick(true).0);
    println!("picked {}", pick(false).0);
    let mut m = Box::new(5);
    *m += 1;
    let r = &mut *m;
    *r *= 2;
    println!("{}", *m);
    let mut s = Box::new(String::from("x"));
    s.push('y');
    let owned: String = *s;
    println!("{owned}");
    let c = if owned.len() > 1 {
        Box::new(Noisy("cond"))
    } else {
        Box::new(Noisy("other"))
    };
    if owned.len() > 5 {
        let _x = *c;
    }
    let mut d = Box::new(Noisy("first"));
    *d = Noisy("second");
    println!("end");
}
