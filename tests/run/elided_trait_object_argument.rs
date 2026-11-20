struct Paint<'paint>(&'paint [u8]);
trait Painter<'paint> {
    fn paint(&mut self, paint: Paint<'paint>);
    fn finish(&mut self);
}
struct Table<'data>(&'data [u8]);
impl<'data> Table<'data> {
    fn paint(&self, painter: &mut dyn Painter<'data>) {
        self.paint_solid(painter);
    }
    fn paint_solid(&self, painter: &mut dyn Painter) {
        painter.finish();
    }
}
struct Counter(usize);
impl Painter<'_> for Counter {
    fn paint(&mut self, paint: Paint<'_>) { self.0 += paint.0.len(); }
    fn finish(&mut self) { self.0 += 1; }
}
fn main(){
    let table = Table(&[1, 2]);
    let mut counter = Counter(0);
    table.paint(&mut counter);
    println!("{}", counter.0);
}
