struct Queue(u32);
struct Guard<'a>(&'a Queue);

impl Drop for Queue {
    fn drop(&mut self) {
        println!("queue");
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        println!("guard {}", self.0.0);
    }
}

impl Queue {
    fn lock(&self) -> Guard<'_> {
        Guard(self)
    }
}

impl Guard<'_> {
    fn read(&mut self) -> Option<u32> {
        Some(self.0.0)
    }
}

fn submit(queue: Option<Box<Queue>>) -> u32 {
    let index = if let Some(queue) = queue {
        queue.lock().read().unwrap_or(0)
    } else {
        0
    };
    index
}

fn main() {
    println!("{}", submit(Some(Box::new(Queue(42)))));
}
