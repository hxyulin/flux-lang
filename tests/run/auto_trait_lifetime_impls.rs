trait Marker { fn value(&self) -> u32; }
struct Number(u32);
impl Marker for Number { fn value(&self) -> u32 { self.0 } }
struct Object<T: ?Sized>(Box<T>);
// SAFETY: the private Marker trait is implemented only by the owned Number type.
unsafe impl Send for Object<dyn Marker> {}
// SAFETY: Number has no interior mutation.
unsafe impl Sync for Object<dyn Marker> {}
struct Static<'a>(&'a u32);
// SAFETY: this immutable reference points to static storage.
unsafe impl Send for Static<'static> {}
fn send<T: Send>(_: &T) {}
fn sync<T: Sync>(_: &T) {}
fn main() {
    let object = Object(Box::new(Number(42)) as Box<dyn Marker>);
    static NUMBER: u32 = 7;
    let reference = Static(&NUMBER);
    send(&object);
    sync(&object);
    send(&reference);
    println!("{} {}", object.0.value(), reference.0);
}
