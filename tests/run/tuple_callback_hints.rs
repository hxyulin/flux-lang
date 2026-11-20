struct Glyph(u32);
impl Glyph {
    fn as_glyph(&self) -> u32 {
        self.0
    }
}
fn apply<F1: Fn(&mut Glyph, u32) -> bool, F2: Fn(&mut Glyph, u32) -> bool>(
    glyph: &mut Glyph,
    matches: (F1, F2),
) -> bool {
    matches.0(glyph, 29) && matches.1(glyph, 31)
}
fn predicate() -> impl Fn(&mut Glyph, u32) -> bool {
    |info: &mut Glyph, value| info.as_glyph() + 2 == value
}
fn main() {
    let mut glyph = Glyph(29);
    let found = apply(
        &mut glyph,
        (|info, val| info.as_glyph() == val, predicate()),
    );
    println!("{found}");
}
