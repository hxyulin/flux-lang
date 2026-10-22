fn named<'a>(x: &'a i32) {
    let _: &'a i32 = x;
}
fn mixed<'a>(x: &'a i32) {
    let _: (&'static i32, &'a i32) = (&0, x);
}
fn main() {}
trait Ref { type Out; }
impl<'a> Ref for &'a i32 { type Out = &'a i32; }
type StaticProjection = <&'static i32 as Ref>::Out;
fn projection(x: &i32) {
    let _: <&'static i32 as Ref>::Out = x;
}
fn projected_alias(x: &i32) {
    let _: StaticProjection = x;
}
