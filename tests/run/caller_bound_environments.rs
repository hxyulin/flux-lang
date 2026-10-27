trait Get<'a> {
    fn get(self) -> &'a u32;
}
impl<'a> Get<'a> for &'a u32 {
    fn get(self) -> &'a u32 {
        self
    }
}
fn get_in_closure<'a, T: Get<'a>>(value: T) -> &'a u32 {
    Some(value).map(|value| value.get()).unwrap()
}
fn nested<'a, T: Get<'a>>(value: T) -> &'a u32 {
    Some(value)
        .map(|value| Some(value).map(|value| value.get()).unwrap())
        .unwrap()
}
fn convert<'a, Q: ?Sized, K>(value: &'a Q) -> K
where
    &'a Q: Into<K>,
{
    value.into()
}
fn convert_in_closure<'a, Q: ?Sized, K>(value: &'a Q) -> K
where
    &'a Q: Into<K>,
{
    Some(value).map(|value| value.into()).unwrap()
}
struct Wrap<T>(T);
impl<'a, T: Get<'a>> Wrap<T> {
    fn run(self) -> &'a u32 {
        Some(self.0).map(|value| value.get()).unwrap()
    }
}
trait Extract<T> {
    fn extract(self) -> T;
}
impl<'a> Extract<&'a mut u32> for &'a mut u32 {
    fn extract(self) -> &'a mut u32 {
        self
    }
}
fn change<'a, T: Extract<&'a mut u32>>(value: T) -> &'a mut u32 {
    Some(value).map(|value| value.extract()).unwrap()
}
fn main() {
    family_consumer();
    let value = 37;
    assert!(core::ptr::eq(get_in_closure(&value), &value));
    assert!(core::ptr::eq(nested(&value), &value));
    assert!(core::ptr::eq(Wrap(&value).run(), &value));
    let source = String::from("borrowed");
    let direct: String = convert(source.as_str());
    let closure: String = convert_in_closure(source.as_str());
    assert_eq!(direct, source);
    assert_eq!(closure, source);
    let mut changed = 7;
    *change(&mut changed) += 5;
    assert_eq!(changed, 12);
    println!("caller contracts {} {} {}", value, closure, changed);
}

trait Part<'data> {
    fn read(&self) -> &'data u32;
}
trait Object<'data> {
    type Symbol<'file>: Part<'data>
    where
        Self: 'file,
        'data: 'file;
    type Symbols<'file>: Iterator<Item = Self::Symbol<'file>>
    where
        Self: 'file,
        'data: 'file;
    fn symbols(&self) -> Self::Symbols<'_>;
    fn find(&self) -> Option<Self::Symbol<'_>> {
        self.symbols().find(|symbol| *symbol.read() == 37)
    }
}
impl<'data> Part<'data> for &'data u32 {
    fn read(&self) -> &'data u32 {
        self
    }
}
struct File<'data>(&'data u32);
impl<'data> Object<'data> for File<'data> {
    type Symbol<'file>
        = &'data u32
    where
        Self: 'file,
        'data: 'file;
    type Symbols<'file>
        = core::array::IntoIter<&'data u32, 1>
    where
        Self: 'file,
        'data: 'file;
    fn symbols(&self) -> Self::Symbols<'_> {
        [self.0].into_iter()
    }
}
fn family_add<'data: 'file, 'file, T: Object<'data>>(_: &'file T, part: &T::Symbol<'file>) -> u32 {
    *part.read()
}
fn family_new<'data, 'file, T: Object<'data>>(file: &'file T, part: &T::Symbol<'file>) -> u32 {
    family_add(file, part)
}
fn family_consumer() {
    let value = 37;
    let file = File(&value);
    let symbol = file.find().unwrap();
    assert!(core::ptr::eq(symbol.read(), &value));
    assert_eq!(family_new(&file, &symbol), 37);
    println!("family contracts {}", *symbol.read());
}
