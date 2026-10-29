struct Job {
    name: &'static str,
    run: fn(&mut u32) -> bool,
}
trait Pass: Sync {
    fn run(&self, n: &mut u32) -> bool;
}
impl Pass for Job {
    fn run(&self, n: &mut u32) -> bool {
        (self.run)(n)
    }
}
fn increment(n: &mut u32) -> bool {
    *n += 1;
    true
}
struct Borrowed<'a, T>(&'a T);
trait Transfer: Send {}
impl<'a, T: Sync> Transfer for Borrowed<'a, T> {}
fn require_transfer<T: Transfer>(_: &T) {}
trait Writable {
    fn bump(&mut self) -> u32;
}
trait Terminal {
    fn terminal(&self) -> bool;
}
impl Terminal for dyn Writable {
    fn terminal(&self) -> bool {
        true
    }
}
trait Raw: Writable + Terminal {}
impl Raw for dyn Writable {}
struct Counter(u32);
impl Writable for Counter {
    fn bump(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}
fn use_raw<T: Raw + ?Sized>(t: &mut T) -> u32 {
    assert!(t.terminal());
    t.bump()
}
trait Get<T> {
    fn get(&self) -> T;
}
impl Get<u8> for Counter {
    fn get(&self) -> u8 {
        self.0 as u8
    }
}
trait RawGet: Get<u8> {}
impl RawGet for dyn Get<u8> {}
fn get<T: RawGet + ?Sized>(t: &T) -> u8 {
    t.get()
}
struct Node<'a> {
    value: &'a u32,
    next: Option<Box<Node<'a>>>,
}
trait Tree: Send + Sync {
    fn total(&self) -> u32;
}
impl Tree for Node<'_> {
    fn total(&self) -> u32 {
        *self.value
            + match &self.next {
                Some(next) => next.total(),
                None => 0,
            }
    }
}
fn tree_total<T: Tree>(value: T) -> u32 {
    value.total()
}
fn main() {
    let job = Job {
        name: "increment",
        run: increment,
    };
    let mut n = 30;
    assert!(Pass::run(&job, &mut n));
    assert_eq!(n, 31);
    assert_eq!(job.name, "increment");
    let data = 47u32;
    let borrowed = Borrowed(&data);
    require_transfer(&borrowed);
    let result = std::thread::scope(|s| s.spawn(move || *borrowed.0).join().unwrap());
    assert_eq!(result, 47);
    let mut counter = Counter(8);
    let object: &mut dyn Writable = &mut counter;
    assert_eq!(use_raw(object), 9);
    let object: &dyn Get<u8> = &counter;
    assert_eq!(get(object), 9);
    let other = 5;
    let tree = Node {
        value: &data,
        next: Some(Box::new(Node {
            value: &other,
            next: None,
        })),
    };
    let tree = tree_total(tree);
    assert_eq!(tree, 52);
    println!("declarations {} {} {} {}", n, result, counter.0, tree);
}
