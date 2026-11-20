trait Class {
    type Thread;
}

struct Worker;
struct Main;

trait MainThreadOnly {}
impl<T: Class<Thread = Main>> MainThreadOnly for T {}

trait Check<Kind> {
    fn check() -> u32;
}

impl<T: ?Sized> Check<()> for T {
    fn check() -> u32 {
        42
    }
}

struct ImplsSend;
impl<T: MainThreadOnly + Send> Check<ImplsSend> for T {
    fn check() -> u32 {
        0
    }
}

struct Object;
impl Class for Object {
    type Thread = Worker;
}

fn main() {
    let check = <Object as Check<_>>::check;
    assert_eq!(check(), 42);
    println!("{}", check());
}
