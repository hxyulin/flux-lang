// Associated constants rejected with the items: duplicates in a trait,
// with functions too (E0428); visibility in a trait's `impl` block
// (E0449) or twice, with functions too (E0201); one without a value in
// an inherent block; a path to one a trait does not have (E0576).

trait Dup {
    const X: i32;
    const X: i32;
    fn Y() {}
    const Y: u8;
}

trait Tr {
    const B: u8 = 1;
}

struct S;

impl Tr for S {
    pub const B: u8 = 2;
    const B: u8 = 3;
    fn B() {}
}

struct V;

impl V {
    const D: i32;
}

fn main() {
    let y = <S as Tr>::Z;
}
