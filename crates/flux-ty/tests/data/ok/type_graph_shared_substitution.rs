type D0<T> = (T, T);
type D1<T> = (D0<T>, D0<T>);
type D2<T> = (D1<T>, D1<T>);
fn identity<T>(x: D2<T>) -> D2<T> { x }
fn main() {
    let x = identity((((true, false), (false, true)), ((true, true), (false, false))));
    let _: bool = x.0.1.0;
}
