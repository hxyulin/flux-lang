// Defaults of structs' and enums' type parameters: too many arguments,
// or none where one has no default (E0107).

struct S<T, U = i32>(T, U);
enum E<A, B = A, C = (A, B)> {
    X(A),
    Y(B),
    Z(C),
}

fn few(s: S) {}
fn many(s: S<i32, u8, u8>) {}
fn many_e(e: E<i32, u8, u8, u8>) {}

fn main() {}
