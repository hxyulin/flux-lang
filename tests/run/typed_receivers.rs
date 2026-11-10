use std::cell::Cell;
use std::rc::Rc;

struct CountDrop(Rc<Cell<u32>>);
impl Drop for CountDrop {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

mod operations {
    pub trait Consume {
        fn consume(self: Box<Self>) -> String;
    }
}
use operations::Consume as _;

struct Item {
    name: String,
    n: u32,
    _drop: CountDrop,
}
impl Item {
    fn get(self: &Self) -> &str {
        &self.name
    }
    fn inc(self: &mut Self) {
        self.n += 1;
    }
    fn into_count(mut self: Box<Self>) -> u32 {
        self.inc();
        self.n
    }
}
impl operations::Consume for Item {
    fn consume(self: Box<Self>) -> String {
        self.name
    }
}

#[repr(transparent)]
struct Items([u32]);
impl Items {
    fn into_values(self: Box<Self>) -> Box<[u32]> {
        unsafe { Box::from_raw(Box::into_raw(self) as *mut [u32]) }
    }
}

struct BorrowedFuture<'a, F: ?Sized> { inner: &'a mut F }
impl<F: Unpin + ?Sized> Unpin for BorrowedFuture<'_, F> {}
impl<F: std::future::Future + Unpin + ?Sized> std::future::Future for BorrowedFuture<'_, F> {
    type Output = F::Output;
    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        std::pin::Pin::new(&mut self.inner).poll(cx)
    }
}

fn main() {
    let mut ready = std::future::ready(31);
    let mut borrowed = BorrowedFuture { inner: &mut ready };
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert_eq!(std::future::Future::poll(std::pin::Pin::new(&mut borrowed), &mut cx), std::task::Poll::Ready(31));
    let drops = Rc::new(Cell::new(0));
    let mut item = Item { name: String::from("owned"), n: 40, _drop: CountDrop(drops.clone()) };
    item.inc();
    assert_eq!(item.get(), "owned");
    let n = Box::new(item).into_count();
    let name = Box::new(Item { name: String::from("moved"), n: 0, _drop: CountDrop(drops.clone()) }).consume();
    let direct = Item::into_count(Box::new(Item { name: String::from("direct"), n: 8, _drop: CountDrop(drops.clone()) }));
    assert_eq!(direct, 9);
    assert_eq!(drops.get(), 3);
    let array = vec![3, 5, 7].into_boxed_slice();
    let wrapper = unsafe { Box::from_raw(Box::into_raw(array) as *mut Items) };
    let values = wrapper.into_values();
    assert_eq!(&*values, &[3, 5, 7]);
    println!("{n} {name} {} {}", values[2], drops.get());
}
