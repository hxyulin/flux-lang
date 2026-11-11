use std::fmt;
trait Extra {
    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result;
}
impl Extra for dyn fmt::Write + '_ {
    fn write_fmt(&mut self, _: fmt::Arguments<'_>) -> fmt::Result {
        fmt::Write::write_str(self, "extra")
    }
}
trait Base {
    fn pick(&self) -> u8 {
        7
    }
}
trait Object: Base {}
impl Base for u8 {}
impl Object for u8 {}
trait Extension {
    fn pick(&self) -> u8 {
        9
    }
}
impl Extension for dyn Object + '_ {}
fn main() {
    let mut buffer = String::new();
    let writer: &mut dyn fmt::Write = &mut buffer;
    write!(writer, "{}", 42).unwrap();
    Extra::write_fmt(writer, format_args!("ignored")).unwrap();
    assert_eq!(buffer, "42extra");
    let object: &dyn Object = &1u8;
    assert_eq!(object.pick(), 7);
    assert_eq!(Extension::pick(object), 9);
    println!("{} {} {}", buffer, object.pick(), Extension::pick(object));
}
