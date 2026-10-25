use core::ops::{Deref, DerefMut};

struct State(u64);
impl State {
    fn callback<'a>(&self, input: &'a u64) -> impl Fn() -> u64 + 'a {
        move || *input
    }
    fn update(&mut self, input: &u64) -> u64 {
        let callback = self.callback(input);
        self.0 += 3;
        callback() + self.0
    }
}

fn signature_only<'a, T, U>(input: &'a u64) -> impl Fn(&(T, U)) -> u64 + 'a {
    move |_| *input
}

struct Guard<T>(T);
impl<T> Deref for Guard<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}
impl<T> DerefMut for Guard<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.0 }
}
fn add(value: &mut u64) { *value += 11; }

fn main() {
    let mut state = State(7);
    println!("opaque {} {}", state.update(&19), state.0);
    let input = 71;
    let callback = signature_only::<&u64, &u64>(&input);
    println!("signature {}", callback(&(&13, &29)));
    let mut value = 3;
    {
        let mut guard = Guard(&mut value);
        add(*guard);
        add(*guard);
    }
    println!("reborrow {value}");
}
