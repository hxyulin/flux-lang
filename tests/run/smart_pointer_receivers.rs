//! Methods whose receiver wraps `Self` in `Box`, `Rc`, `Arc` or `Pin`,
//! called directly, through `&Arc<Self>`, and on trait objects.
use std::any::Any;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

mod direct {
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

    pub fn run() {
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
}

mod objects {
    use std::rc::Rc;
    use std::sync::Arc;

    trait Shape {
        fn area(&self) -> i32;
        fn boxed(self: Box<Self>) -> i32;
        fn shared(self: Rc<Self>) -> i32;
        fn atomic(self: Arc<Self>) -> i32;
    }

    struct Square(i32);

    impl Shape for Square {
        fn area(&self) -> i32 {
            self.0 * self.0
        }
        fn boxed(self: Box<Self>) -> i32 {
            self.area() + 1
        }
        fn shared(self: Rc<Self>) -> i32 {
            self.area() + 2
        }
        fn atomic(self: Arc<Self>) -> i32 {
            self.area() + Arc::strong_count(&self) as i32 * 100
        }
    }

    pub fn run() {
        let b: Box<dyn Shape> = Box::new(Square(2));
        println!("{}", b.boxed());
        let r: Rc<dyn Shape> = Rc::new(Square(3));
        println!("{}", r.clone().shared());
        println!("{}", r.area());
        let a: Arc<dyn Shape> = Arc::new(Square(4));
        let a2 = a.clone();
        println!("{}", a.atomic());
        println!("{}", a2.atomic());
    }
}

trait Downcast: Any {
    fn into_any_rc(self: Rc<Self>) -> Rc<dyn Any>;
    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
    fn name(&self) -> &'static str;
}

struct Cat(u8);

impl Downcast for Cat {
    fn into_any_rc(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
    fn name(&self) -> &'static str {
        "cat"
    }
}

trait Step {
    fn step(self: Pin<&mut Self>) -> u32;
}

struct Count(u32);

impl Step for Count {
    fn step(mut self: Pin<&mut Self>) -> u32 {
        self.0 += 1;
        self.0
    }
}

fn main() {
    direct::run();
    objects::run();
    let d: Rc<dyn Downcast> = Rc::new(Cat(7));
    println!("{}", d.name());
    let any = d.into_any_rc();
    println!("{}", any.downcast_ref::<Cat>().map_or(0, |c| c.0));
    let a: Arc<dyn Downcast> = Arc::new(Cat(9));
    let any = a.into_any_arc();
    println!("{}", any.is::<Cat>());
    let mut c = Count(1);
    let r: &mut dyn Step = &mut c;
    let mut p: Pin<&mut dyn Step> = unsafe { Pin::new_unchecked(r) };
    println!("{}", p.as_mut().step());
    println!("{}", p.as_mut().step());
}