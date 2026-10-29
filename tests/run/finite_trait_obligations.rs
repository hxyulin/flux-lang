static GLOBAL: u32 = 7;
trait Read<'a> {
    fn read(&self) -> &'a u32;
}
struct Leaf<'a>(&'a u32);
impl<'a> Read<'a> for Leaf<'a> {
    fn read(&self) -> &'a u32 {
        self.0
    }
}
struct Wrap<P>(P);
impl<'a, P: Read<'a>> Read<'a> for Wrap<P> {
    fn read(&self) -> &'a u32 {
        self.0.read()
    }
}
struct Pair<P, Q>(P, Q);
impl<'a, P: Read<'a>, Q: Read<'a>> Read<'a> for Pair<P, Q> {
    fn read(&self) -> &'a u32 {
        self.0.read()
    }
}
fn need<'a, P: Read<'a>>(p: &P, _: &'a u32) -> &'a u32 {
    p.read()
}
fn identity<'a>(x: &'a u32) -> &'a u32 {
    x
}
struct Map<P, F>(P, F);
impl<'a, P, F> Read<'a> for Map<P, F>
where
    P: Read<'a>,
    F: Fn(&'a u32) -> &'a u32,
{
    fn read(&self) -> &'a u32 {
        self.1(self.0.read())
    }
}
fn main() {
    let value = 17;
    let parser = Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(
        Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(
            Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(
                Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(
                    Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(
                        Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Leaf(&value)))))))))),
                    ))))))))))),
                ))))))))))),
            ))))))))))),
        ))))))))))),
    )))))))))));
    let found = need(&parser, &value);
    assert_eq!(found as *const u32, &value as *const u32);
    let mapped = Map(
        Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(
            Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(
                Wrap(Wrap(Leaf(&value))),
            ))))))))))),
        ))))))))))),
        identity,
    );
    let answer = need(&mapped, &value);
    assert_eq!(*answer, 17);
    let sibling = Pair(
        Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(
            Wrap(Leaf(&GLOBAL)),
        ))))))))))),
        Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(Wrap(
            Wrap(Leaf(&value)),
        ))))))))))),
    );
    assert_eq!(*need(&sibling, &value), 7);
    println!(
        "finite obligations {} {} {}",
        found,
        answer,
        need(&sibling, &value)
    );
}
