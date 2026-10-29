use std::error::Error;
use std::panic::UnwindSafe;
trait Sealed {}
trait AsDynError<'a>: Sealed {
    fn as_dyn_error(&self) -> &(dyn Error + 'a);
}
impl<'a, T: Error + 'a> AsDynError<'a> for T {
    fn as_dyn_error(&self) -> &(dyn Error + 'a) {
        self
    }
}
impl<T: Error> Sealed for T {}
impl<'a> AsDynError<'a> for dyn Error + 'a {
    fn as_dyn_error(&self) -> &(dyn Error + 'a) {
        self
    }
}
impl Sealed for dyn Error + '_ {}
impl<'a> AsDynError<'a> for dyn Error + Send + 'a {
    fn as_dyn_error(&self) -> &(dyn Error + 'a) {
        self
    }
}
impl Sealed for dyn Error + Send + '_ {}
impl<'a> AsDynError<'a> for dyn Error + Send + Sync + 'a {
    fn as_dyn_error(&self) -> &(dyn Error + 'a) {
        self
    }
}
impl Sealed for dyn Error + Send + Sync + '_ {}
impl<'a> AsDynError<'a> for dyn Error + Send + Sync + UnwindSafe + 'a {
    fn as_dyn_error(&self) -> &(dyn Error + 'a) {
        self
    }
}
impl Sealed for dyn Error + Send + Sync + UnwindSafe + '_ {}
#[derive(Debug)]
struct Borrowed<'a>(&'a str);
impl std::fmt::Display for Borrowed<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl Error for Borrowed<'_> {}
fn convert<'data: 'borrow, 'borrow, T: AsDynError<'data> + ?Sized>(
    value: &'borrow T,
) -> &'borrow (dyn Error + 'data) {
    value.as_dyn_error()
}
trait Node {
    fn value(&self) -> u32;
}
impl Node for u32 {
    fn value(&self) -> u32 {
        *self
    }
}
trait Visit<'doc> {
    fn visit(&mut self, node: &'doc dyn Node);
    fn default_visit(&mut self, node: &'doc dyn Node) {
        self.visit(node);
    }
}
struct Collector<'doc> {
    nodes: Vec<&'doc dyn Node>,
}
impl<'doc> Visit<'doc> for Collector<'doc> {
    fn visit(&mut self, node: &'doc dyn Node) {
        self.nodes.push(node);
    }
}
fn apply<'doc, V: Visit<'doc> + ?Sized>(visitor: &mut V, node: &'doc dyn Node) {
    visitor.default_visit(node);
}
fn main() {
    let text = String::from("borrowed error");
    let error = Borrowed(&text);
    assert_eq!(format!("{}", convert(&error)), text);
    let bare: &dyn Error = &error;
    let send: &(dyn Error + Send) = &error;
    let sync: &(dyn Error + Send + Sync) = &error;
    let unwind: &(dyn Error + Send + Sync + UnwindSafe) = &error;
    for value in [convert(bare), convert(send), convert(sync), convert(unwind)] {
        assert_eq!(format!("{}", value), "borrowed error");
    }
    let left = 7u32;
    let right = 11u32;
    let mut visitor = Collector { nodes: Vec::new() };
    apply(&mut visitor, &left);
    apply(&mut visitor, &right);
    assert_eq!(visitor.nodes[0].value(), 7);
    assert_eq!(visitor.nodes[1].value(), 11);
    println!(
        "objects {} {}",
        convert(&error),
        visitor.nodes.iter().map(|x| x.value()).sum::<u32>()
    );
}
