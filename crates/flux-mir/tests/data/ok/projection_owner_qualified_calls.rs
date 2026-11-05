// serde's `<A::Error as Error>::duplicate_field(..)`: a qualified call whose
// type is a shorthand projection with an owner lifetime from the caller's
// bound. Its lifetimes are inferred, also as a constructor's type argument
// next to a written lifetime.
pub trait Error: Sized {
    fn duplicate_field(field: &'static str) -> Self;
    fn code(&self) -> i32;
}

pub trait MapAccess<'de> {
    type Error: Error;
    fn next(&mut self) -> Result<Option<&'de u8>, Self::Error>;
}

pub struct Holder<'a, E> {
    name: &'a str,
    error: Option<E>,
}

struct Fail;

impl Error for Fail {
    fn duplicate_field(field: &'static str) -> Self {
        println!("duplicate field `{field}`");
        Fail
    }
    fn code(&self) -> i32 {
        1
    }
}

struct Map<'a>(&'a [u8]);

impl<'de> MapAccess<'de> for Map<'de> {
    type Error = Fail;
    fn next(&mut self) -> Result<Option<&'de u8>, Fail> {
        let first = self.0.first();
        if first.is_some() {
            self.0 = &self.0[1..];
        }
        Ok(first)
    }
}

fn visit_map<'de, A>(mut map: A) -> Result<&'de u8, A::Error>
where
    A: MapAccess<'de>,
{
    if let Some(v) = map.next()? {
        return Ok(v);
    }
    Err(<A::Error as Error>::duplicate_field("secs"))
}

fn written<'de, A: MapAccess<'de>>() -> i32 {
    let h = Holder::<'static, A::Error> { name: "holder", error: None };
    h.name.len() as i32 + h.error.map_or(34, |e| e.code())
}

fn main() {
    let data = [4u8];
    println!("{}", visit_map(Map(&data)).ok().copied().unwrap_or(0));
    let code = match visit_map(Map(&[])) {
        Ok(_) => 0,
        Err(e) => e.code(),
    };
    std::process::exit(code + written::<Map<'_>>());
}
