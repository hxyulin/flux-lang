struct Field(u32);
struct Outcome(Vec<Field>);
impl Field {
    fn to_outer<'a>(&'a self, outcome: &'a Outcome) -> (&'a u32, &'a u32) {
        (&self.0, &outcome.0[0].0)
    }
}
impl Outcome {
    fn iter(&self) -> impl Iterator<Item = (&u32, &u32)> {
        self.0.iter().filter_map(|item| Some(item.to_outer(self)))
    }
}
fn main() {
    let outcome = Outcome(vec![Field(42)]);
    println!("{:?}", outcome.iter().next());
}
