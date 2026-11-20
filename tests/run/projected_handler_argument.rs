trait Handler {
    type Message: 'static;

    fn handle(&mut self, message: Self::Message);
}

trait Application<Message> {
    fn apply(&mut self, message: Message);
}

struct Driver<'a, H: Handler>(&'a mut H);

impl<H: Handler> Application<H::Message> for Driver<'_, H> {
    fn apply(&mut self, message: H::Message) {
        self.0.handle(message);
    }
}

struct EventLoop<M>(M);

impl<M: 'static> EventLoop<M> {
    fn run_app<A: Application<M>>(self, app: &mut A) {
        app.apply(self.0);
    }
}

fn run<M: 'static, H: Handler<Message = M>>(event_loop: EventLoop<M>, handler: &mut H) {
    let mut driver = Driver(handler);
    event_loop.run_app(&mut driver);
}

struct Counter(u32);

impl Handler for Counter {
    type Message = u32;

    fn handle(&mut self, message: Self::Message) {
        self.0 += message;
    }
}

fn main() {
    let mut counter = Counter(10);
    run(EventLoop(32), &mut counter);
    assert_eq!(counter.0, 42);
    println!("{}", counter.0);
}
