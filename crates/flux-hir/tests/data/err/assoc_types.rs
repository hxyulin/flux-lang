// Associated types: items rejected while lowering.

struct S;

// Two associated types with the same name, in a trait and in an impl.
trait Tr {
    type Out;
    type Out;
}

trait Two {
    type A;
}

impl Two for S {
    type A = i32;
    type A = u8;
}

// Associated type defaults and inherent associated types are unstable.
trait WithDefault {
    type Out = i32;
}

impl S {
    type Inherent = i32;
}

// Trait parameters with defaults must come last.
trait Defaults<T = i32, U> {
    fn m(&self, t: T, u: U);
}

trait Defaults2<T, U = bool, V> {
    fn m(&self, t: T, u: U, v: V);
}

// An associated type without a value in an impl (which still counts as
// defined).
trait Three {
    type A;
}

impl Three for S {
    type A;
}

// E0229: bindings in expression paths.
struct G<T>(T);

fn binding_in_expr() {
    let _ = G::<Out = i32>(1);
}

fn main() {}
