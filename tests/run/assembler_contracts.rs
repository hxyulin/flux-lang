use std::borrow::Cow;
use std::cell::Cell;
use std::marker::PhantomData;

#[derive(Debug)]
struct Failure<'a>(&'a str);
impl std::fmt::Display for Failure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Failure<'_> {}
fn boxed<'a>(text: &'a str) -> Result<(), Box<dyn std::error::Error + 'a>> {
    Err(Failure(text))?
}
struct Payload<'a>(&'a Cell<u32>);
impl Drop for Payload<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn as_text<T: AsRef<str>>(x: &T) -> &str {
    x.as_ref()
}
fn main() {
    let _: PhantomData<str> = PhantomData::default();
    let drops = Cell::new(0);
    let value: Result<Option<Payload<'_>>, u32> = Ok(Some(Payload(&drops)));
    drop(value.transpose().unwrap().unwrap());
    assert_eq!(drops.get(), 1);
    assert_eq!(Ok::<Option<u8>, u32>(None).transpose(), None);
    assert_eq!(Err::<Option<u8>, u32>(9).transpose(), Some(Err(9)));
    let text = String::from("data");
    assert_eq!(Ok::<_, u32>(&text).cloned().unwrap(), "data");
    assert_eq!(Err::<&String, _>(8).cloned(), Err(8));
    assert_eq!(as_text(&text), "data");
    for borrowed in [true, false] {
        let cow: Cow<'_, str> = if borrowed {
            Cow::Borrowed("λdata")
        } else {
            Cow::Owned(String::from("λdata"))
        };
        assert!(cow == "λdata");
        assert!("λdata" == cow);
        assert!(cow != "other");
        assert!(cow == Cow::Borrowed("λdata"));
        assert!(cow == *"λdata");
    }
    let cow: Cow<'_, [u8]> = vec![1, 2, 3].into();
    assert_eq!(&*cow, &[1, 2, 3]);
    let array: [u8; 3] = (&mut [4, 5, 6][..]).try_into().unwrap();
    assert_eq!(array, [4, 5, 6]);
    let bad: Result<[u8; 2], _> = (&mut [1, 2, 3][..]).try_into();
    assert!(bad.is_err());
    assert_eq!(" \t\nλ \r".trim_ascii(), "λ");
    assert_eq!("\u{a0}x\u{a0}".trim_ascii(), "\u{a0}x\u{a0}");
    let message = String::from("borrowed");
    let error = boxed(&message).unwrap_err();
    assert!(error.source().is_none());
    assert_eq!(error.to_string(), "borrowed");
    assert_eq!(format!("{error:?}"), "Failure(\"borrowed\")");
    println!("contracts {} {} {:?}", drops.get(), as_text(&text), array);
}
