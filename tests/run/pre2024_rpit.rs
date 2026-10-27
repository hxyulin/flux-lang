fn values<'a>(xs: &'a [u32]) -> impl Iterator<Item = &'a u32> {
    xs.iter()
}
fn callback(x: &u32) -> impl Fn() -> u32 + '_ {
    move || *x
}
fn precise<'a>(_: &'a u32) -> impl Fn() -> u32 + use<> {
    || 19
}
fn main() {
    let xs = [7, 11, 13];
    assert_eq!(values(&xs).copied().sum::<u32>(), 31);
    assert_eq!(callback(&xs[1])(), 11);
    let independent;
    {
        let local = 23;
        independent = precise(&local);
    }
    println!("rpit {} {}", callback(&xs[1])(), independent());
}
