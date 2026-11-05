// Receivers that wrap Self in Arc, Rc and Pin<Box<_>>, and methods called
// through them.
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

struct Counter {
    n: i32,
}

impl Counter {
    fn by_arc(self: Arc<Self>) -> i32 {
        self.n
    }
    fn by_arc_ref(self: &Arc<Self>) -> Arc<Self> {
        self.clone()
    }
    fn by_rc(self: Rc<Self>) -> i32 {
        self.n + 1
    }
    fn by_pin_box_ref(self: &Pin<Box<Self>>) -> i32 {
        self.n + 2
    }
    fn get(&self) -> i32 {
        self.n
    }
}

trait Wake {
    fn wake(self: Arc<Self>) -> i32;
    fn wake_by_ref(self: &Arc<Self>) -> i32 {
        self.clone().wake()
    }
}

impl Wake for Counter {
    fn wake(self: Arc<Self>) -> i32 {
        self.get() * 10
    }
}

struct Holder {
    inner: Arc<Counter>,
}

fn generic<W: Wake>(w: Arc<W>) -> i32 {
    w.wake_by_ref() + w.wake()
}

fn main() {
    let a = Arc::new(Counter { n: 3 });
    let b = a.by_arc_ref();
    println!("{}", b.by_arc());
    println!("{}", a.wake_by_ref());
    let h = Holder { inner: a.clone() };
    println!("{}", h.inner.by_arc_ref().get());
    let r = &h.inner;
    println!("{}", r.wake_by_ref());
    println!("{}", Rc::new(Counter { n: 4 }).by_rc());
    let p = Pin::new(Box::new(Counter { n: 5 }));
    println!("{}", p.by_pin_box_ref());
    println!("{}", generic(a));
    println!("{}", Counter::by_arc(Arc::new(Counter { n: 6 })));
}
