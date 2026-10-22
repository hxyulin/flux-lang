struct Container(Vec<i32>);
impl Container {
    fn values(&self) -> &[i32] { &self.0 }
}
fn change(c: &Container) {
    c.values().fill(1);
}
