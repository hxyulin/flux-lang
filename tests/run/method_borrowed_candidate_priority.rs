trait A {
    fn pick(&self) -> u8;
}

trait B {
    fn pick(self) -> u8;
}

struct S;

impl A for S {
    fn pick(&self) -> u8 {
        1
    }
}

impl<T> B for &T {
    fn pick(self) -> u8 {
        2
    }
}

fn borrowed<T: A>(x: &T) -> u8 {
    x.pick()
}

fn owned<T: A>(x: T) -> u8 {
    x.pick()
}

fn exact_018() {
    println!("{} {}", borrowed(&S), owned(S));
}

// The same receiver type must also preserve inherent priority.
struct Inherent;
impl Inherent {
    fn pick(&self) -> u8 {
        3
    }
}

trait Mutate {
    fn bump(&mut self) -> u8;
}
trait BorrowedMutate {
    fn bump(self) -> u8;
}
struct Counter(u8);
impl Mutate for Counter {
    fn bump(&mut self) -> u8 {
        self.0 += 1;
        self.0
    }
}
impl<T> BorrowedMutate for &mut T {
    fn bump(self) -> u8 {
        99
    }
}
fn reborrow<T: Mutate>(x: &mut T) -> u8 {
    let first = x.bump();
    first + x.bump()
}
// A by-value mutable-reference candidate precedes a shared reborrow.
trait Shared {
    fn choose(&self) -> u8;
}
trait Take {
    fn choose(self) -> u8;
}
impl Shared for Counter {
    fn choose(&self) -> u8 {
        5
    }
}
impl<T> Take for &mut T {
    fn choose(self) -> u8 {
        6
    }
}
fn take<T: Shared>(x: &mut T) -> u8 {
    x.choose()
}

// An earlier candidate on `&&T` still wins before reaching the bound on T.
fn nested<T: A>(x: &&T) -> u8 {
    x.pick()
}
trait Child: A {}
impl Child for S {}
fn inherited<T: Child>(x: &T) -> u8 {
    x.pick()
}
fn main() {
    exact_018();
    deref_unsize::run();
    println!("{} {} {}", (&Inherent).pick(), nested(&&S), inherited(&S));
    let mut c = Counter(10);
    println!("{}", reborrow(&mut c));
    let chosen = take(&mut c);
    println!("{} {}", c.0, chosen);
}

mod deref_unsize {
    use core::ops::{Deref, DerefMut};
    struct W {
        v: [u8; 3],
        n: u8,
    }
    impl Deref for W {
        type Target = [u8; 3];
        fn deref(&self) -> &[u8; 3] {
            &self.v
        }
    }
    impl DerefMut for W {
        fn deref_mut(&mut self) -> &mut [u8; 3] {
            self.n += 1;
            &mut self.v
        }
    }
    trait Edit {
        type Out;
        fn edit(self) -> Self::Out;
    }
    impl<'a> Edit for &'a mut [u8] {
        type Out = &'a mut u8;
        fn edit(self) -> Self::Out {
            &mut self[1]
        }
    }
    trait View {
        type Out;
        fn view(self) -> Self::Out;
    }
    impl<'a> View for &'a [u8] {
        type Out = &'a u8;
        fn view(self) -> Self::Out {
            &self[1]
        }
    }
    pub fn run() {
        let mut w = W { v: [3, 5, 7], n: 0 };
        *w.edit() += 10;
        println!("{} {}", w.view(), w.n);
    }
}
