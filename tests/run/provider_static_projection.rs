trait Storage<'a> {
    type Container;
}
impl<'a> Storage<'a> for u8 {
    type Container = Vec<&'a u8>;
}
struct Bucket<'a, K: Storage<'a>> {
    elements: K::Container,
}
trait Data<'a>: 'static {}
impl Data<'_> for Bucket<'static, u8> {}
trait Marker: 'static {
    type Value: for<'a> Data<'a>;
}
struct Tag;
struct Payload<M: Marker>(Option<&'static M::Value>);
struct Concrete(Payload<Tag>);

struct Consumer<C>(C);
impl<C: AsRef<Concrete>> Consumer<C> {
    fn get(&self) -> &Concrete {
        self.0.as_ref()
    }
}

// The projected data type is declared after the impl that needs it.
impl Marker for Tag {
    type Value = Bucket<'static, u8>;
}

trait Provider<M: Marker> {
    fn load(&self) -> Payload<M>;
}

struct P;
impl Provider<Tag> for P {
    fn load(&self) -> Payload<Tag> {
        Payload(None)
    }
}

fn abstract_target<C: AsRef<Concrete>>() {}

fn main() {
    assert!(P.load().0.is_none());
}
