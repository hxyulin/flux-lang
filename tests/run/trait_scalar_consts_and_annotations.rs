// Reductions preserve the upstream method and local annotation shapes. The
// independent Flavor<'de>: 'de declaration boundary is not exercised here.
enum Reference<'de, 's, T: ?Sized + 'static> {
    Borrowed(&'de T),
    Copied(&'s T),
}
trait Sealed {}
trait Read<'de>: Sealed {
    const SHOULD_EARLY_RETURN: bool;
    fn parse<'s>(&'s mut self, scratch: &'s mut String) -> Reference<'de, 's, str>;
}
struct SliceRead<'de> {
    text: &'de str,
    copied: bool,
}
impl Sealed for SliceRead<'_> {}
impl<'de> Read<'de> for SliceRead<'de> {
    const SHOULD_EARLY_RETURN: bool = false;
    fn parse<'s>(&'s mut self, scratch: &'s mut String) -> Reference<'de, 's, str> {
        if self.copied {
            scratch.clear();
            scratch.push_str(self.text);
            scratch.push_str(" copied");
            Reference::Copied(scratch.as_str())
        } else {
            Reference::Borrowed(self.text)
        }
    }
}
fn borrowed<'de>(input: &'de str) -> &'de str {
    let mut reader = SliceRead {
        text: input,
        copied: false,
    };
    let mut scratch = String::new();
    match reader.parse(&mut scratch) {
        Reference::Borrowed(value) => {
            let value: &'de str = value;
            value
        }
        Reference::Copied(_) => panic!("wrong origin"),
    }
}
trait Take<'de> {
    const READY: bool = true;
    fn take(&mut self) -> &'de [u8];
}
struct Bytes<'de>(&'de [u8]);
impl<'de> Take<'de> for Bytes<'de> {
    fn take(&mut self) -> &'de [u8] {
        self.0
    }
}
struct Deserializer<'de, F> {
    flavor: F,
    marker: std::marker::PhantomData<&'de ()>,
}
impl<'de, F: Take<'de>> Deserializer<'de, F> {
    fn read(&mut self) -> &'de [u8] {
        let bytes: &'de [u8] = self.flavor.take();
        bytes
    }
}
fn flag<'de, R: Read<'de>>(_: &R) -> bool {
    R::SHOULD_EARLY_RETURN
}
fn ready<'de, F: Take<'de>>(_: &F) -> bool {
    F::READY
}
fn main() {
    let text = String::from("hello");
    let output = borrowed(&text);
    assert_eq!(output.as_ptr(), text.as_ptr());
    assert_eq!(output, "hello");
    let mut reader = SliceRead {
        text: &text,
        copied: true,
    };
    let mut scratch = String::new();
    match reader.parse(&mut scratch) {
        Reference::Copied(value) => assert_eq!(value, "hello copied"),
        _ => panic!("wrong origin"),
    }
    assert!(!flag(&reader));
    assert!(ready(&Bytes(b"x")));
    let data = [3u8, 5, 8];
    let result = {
        let mut d = Deserializer {
            flavor: Bytes(&data),
            marker: std::marker::PhantomData,
        };
        d.read()
    };
    assert_eq!(result, &data);
    assert_eq!(result.as_ptr(), data.as_ptr());
    println!("read {} {:?} {}", output, result, scratch);
}
