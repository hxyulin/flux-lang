// Repeated monomorphizations share static graphs, including cycles and
// references into fields. Function pointers and vtable entries must survive.
struct Node {
    run: fn(u32) -> u32,
    next: &'static Node,
}

fn add(x: u32) -> u32 { x + 7 }
fn double(x: u32) -> u32 { x * 2 }
static FIRST: Node = Node { run: add, next: &SECOND };
static SECOND: Node = Node { run: double, next: &FIRST };
static CALLBACK: &fn(u32) -> u32 = &SECOND.run;
static EMPTY: [u32; 3] = [11, 13, 17];

trait Base: Sync {
    fn base(&self) -> u32;
}
trait Action: Base {
    fn apply(&self, value: u32) -> u32;
}
struct Handler(u32);
impl Base for Handler {
    fn base(&self) -> u32 { self.0 }
}
impl Action for Handler {
    fn apply(&self, value: u32) -> u32 { value + self.0 }
}
impl Drop for Handler {
    fn drop(&mut self) { println!("unexpected static drop"); }
}
static HANDLER: Handler = Handler(19);
static ACTIONS: [&dyn Action; 2] = [&HANDLER, &Handler(23)];

fn exercise<T>(input: u32) -> u32 {
    let first = &FIRST;
    let second = first.next;
    assert!(core::ptr::eq(second.next, first));
    (first.run)(input) + (second.run)(input) + (*CALLBACK)(input)
        + ACTIONS[0].apply(input) + ACTIONS[1].base()
        + EMPTY[core::mem::size_of::<T>() % EMPTY.len()]
}

fn main() {
    println!("{} {} {}", exercise::<u8>(3), exercise::<u16>(5), exercise::<u64>(7));
}
