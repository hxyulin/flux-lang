#[derive(Debug)]
struct Value(u32);
type Values<'a> = Box<dyn Iterator<Item = &'a Value> + 'a>;
type ValuesMut<'a> = Box<dyn Iterator<Item = &'a mut Value> + 'a>;
type Entries<'a> = Box<dyn Iterator<Item = (&'a str, &'a Value)> + 'a>;
struct KeyMut<'a>(&'a mut String);
type EntriesMut<'a> = Box<dyn Iterator<Item = (KeyMut<'a>, &'a mut Value)> + 'a>;
fn values(input: &[Value]) -> Values<'_> {
    Box::new(input.iter())
}
fn values_mut(input: &mut [Value]) -> ValuesMut<'_> {
    Box::new(input.iter_mut())
}
fn entries(input: &[(String, Value)]) -> Entries<'_> {
    Box::new(input.iter().map(|(key, value)| (key.as_str(), value)))
}
fn entries_mut(input: &mut [(String, Value)]) -> EntriesMut<'_> {
    Box::new(input.iter_mut().map(|(key, value)| (KeyMut(key), value)))
}
trait Pair {
    type A;
    type B;
    fn a(&self) -> Self::A;
    fn b(&self) -> Self::B;
}
struct PairValue<'a, 'b>(&'a Value, &'b Value);
impl<'a, 'b> Pair for PairValue<'a, 'b> {
    type A = &'a Value;
    type B = &'b Value;
    fn a(&self) -> Self::A {
        self.0
    }
    fn b(&self) -> Self::B {
        self.1
    }
}
fn pair<'a: 'o, 'b: 'o, 'o>(
    a: &'a Value,
    b: &'b Value,
) -> Box<dyn Pair<B = &'b Value, A = &'a Value> + 'o> {
    Box::new(PairValue(a, b))
}
fn main() {
    ordinary_object_calls();
    let mut input = vec![Value(3), Value(5), Value(7)];
    let escaped = {
        let mut iter = values(&input);
        iter.next().unwrap()
    };
    assert_eq!(escaped.0, 3);
    let total: u32 = values(&input).map(|value| value.0).sum();
    assert_eq!(total, 15);
    {
        let mut iter = values_mut(&mut input);
        let first = iter.next().unwrap();
        let second = iter.next().unwrap();
        first.0 += 10;
        second.0 += 20;
    }
    let tail = {
        let mut iter = values_mut(&mut input);
        iter.next().unwrap()
    };
    tail.0 += 1;
    assert_eq!(input[0].0, 14);
    assert_eq!(input[1].0, 25);
    let mut table = vec![
        (String::from("alpha"), Value(11)),
        (String::from("beta"), Value(19)),
    ];
    let saved = {
        let mut iter = entries(&table);
        iter.next().unwrap()
    };
    assert_eq!(saved.0, "alpha");
    assert_eq!(saved.1.0, 11);
    for (key, value) in entries_mut(&mut table) {
        key.0.push('!');
        value.0 += 1;
    }
    let mut it = entries(&table);
    let first = it.next().unwrap();
    let second = it.next().unwrap();
    assert_eq!((first.0, first.1.0), ("alpha!", 12));
    assert_eq!((second.0, second.1.0), ("beta!", 20));
    let left = Value(31);
    let right = Value(43);
    let p = pair(&left, &right);
    assert_eq!(p.a().0, 31);
    assert_eq!(p.b().0, 43);
    println!(
        "object argument rows {total} {} {} {} {} {} {}",
        input[0].0,
        input[1].0,
        first.0,
        second.0,
        p.a().0,
        p.b().0
    );
}

trait GenericGet<T> {
    fn echo(&self, value: T) -> T;
}
trait GenericRaw<T>: GenericGet<T> {}
impl<T> GenericRaw<T> for dyn GenericGet<T> {}
struct Identity;
impl<T> GenericGet<T> for Identity {
    fn echo(&self, value: T) -> T {
        value
    }
}
fn principal_call<T: GenericGet<U> + ?Sized, U>(object: &T, value: U) -> U {
    object.echo(value)
}
fn user_call<T: GenericRaw<U> + ?Sized, U>(object: &T, value: U) -> U {
    object.echo(value)
}
fn ordinary_object_calls() {
    let left = 197;
    let right = 211;
    let identity = Identity;
    let object: &dyn GenericGet<&u32> = &identity;
    let p = principal_call(object, &left);
    let u = user_call(object, &right);
    assert_eq!(p as *const u32, &left as *const u32);
    assert_eq!(u as *const u32, &right as *const u32);
    let pair_object: &dyn GenericGet<(&u32, &u32)> = &identity;
    let pair = user_call(pair_object, (&left, &right));
    assert_eq!(pair.0 as *const u32, &left as *const u32);
    assert_eq!(pair.1 as *const u32, &right as *const u32);
    let mut value = 223;
    let mut_object: &dyn GenericGet<&mut u32> = &identity;
    *user_call(mut_object, &mut value) += 1;
    assert_eq!(value, 224);
    println!("ordinary object calls {} {} {}", p, u, value);
}
