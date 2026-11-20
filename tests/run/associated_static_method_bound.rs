struct Event<T: 'static>(T);

trait Handler<T: 'static> {
    fn handle(&mut self, event: T);
}

trait Dispatch {
    type UserEvent: 'static;

    fn run<F>(&mut self, callback: F)
    where
        F: FnMut(Event<Self::UserEvent>, &u32);

    fn run_app<A: Handler<Self::UserEvent>>(&mut self, app: &mut A) {
        self.run(|event, _| dispatch(app, event));
    }
}

fn dispatch<T: 'static, A: Handler<T>>(app: &mut A, event: Event<T>) {
    app.handle(event.0);
}

struct Push;
impl Dispatch for Push {
    type UserEvent = u32;
    fn run<F: FnMut(Event<u32>, &u32)>(&mut self, mut callback: F) {
        let value = 42;
        callback(Event(value), &value);
    }
}

impl Handler<u32> for u32 {
    fn handle(&mut self, event: u32) {
        *self = event;
    }
}

fn main() {
    let mut value = 0;
    Push.run_app(&mut value);
    println!("{value}");
}
