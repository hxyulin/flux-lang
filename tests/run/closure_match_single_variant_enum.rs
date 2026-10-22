//! Regression for difftest 013: closure pattern captures.

enum E {
    V,
}

fn main() {
    nested();
    let v = E::V;
    let c = || match v {
        E::V => 1,
    };
    println!("{}", c());
}

enum One<T> {
    V(T),
}

struct D(u8);
impl Drop for D {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn nested() {
    // Neither the enum nor its wildcard fields are captured: moving it
    // before calling the closure must be allowed, with drops observable.
    let v = (One::V((One::V(D(7)), 3)),);
    let c = || match v {
        (One::V((One::V(_), _)),) => 2,
    };
    drop(v);
    println!("{}", c());

    // Reading a field captures the enum and still needs its downcast.
    let v = One::V(D(8));
    let c = || match v {
        One::V(ref x) if x.0 > 0 => x.0,
        One::V(_) => 0,
    };
    println!("{}", c());
}
