use std::cell::Cell;
use std::io::{self, Write};
trait Maker<'a> {
    type Writer: Write+'a;
    fn make(&'a self)->Self::Writer;
}
struct Counter(Cell<usize>);
struct Borrowed<'a>(&'a Cell<usize>);
impl Write for Borrowed<'_> {
    fn write(&mut self, bytes:&[u8])->io::Result<usize>{self.0.set(self.0.get()+bytes.len());Ok(bytes.len())}
    fn flush(&mut self)->io::Result<()>{Ok(())}
}
impl<'a> Maker<'a> for Counter {
    type Writer=Box<dyn Write+'a>;
    fn make(&'a self)->Self::Writer{Box::new(Borrowed(&self.0))}
}
fn write(value:&dyn for<'a> Maker<'a,Writer=Box<dyn Write+'a>>) {
    let mut writer=value.make();
    writer.write_all(b"abc").unwrap();
    writer.flush().unwrap();
}
fn main(){
    let count=Counter(Cell::new(0));
    write(&count);write(&count);
    println!("{}",count.0.get());
    let boxed:Box<dyn for<'a> Maker<'a,Writer=Box<dyn Write+'a>>>=Box::new(Counter(Cell::new(0)));
    write(&*boxed);
    println!("writer object passed");
}
