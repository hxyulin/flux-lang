struct P(i32, i32);
struct U;
enum E {
    A,
}

type PA = P;
type UA = U;
type EA = E;
type NoBody;
type WithBounds: Copy = i32;

fn main() {
    let a = PA(1, 2);
    let u = UA;
    let PA(x, y) = P(1, 2);
    let e = EA::B;
}
