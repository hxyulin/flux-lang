trait Tr {
    fn f(&self, x: &i32) -> &i32;
    fn g<'a>(&self, x: &'a i32, y: &'a i32) -> &'a i32;
    fn h(&self, x: &i32, y: &i32) -> i32;
}

struct S(i32);

impl Tr for S {
    fn f<'a>(&self, x: &'a i32) -> &'a i32 {
        x
    }
    fn g<'b>(&self, x: &'b i32, _y: &'b i32) -> &'b i32 {
        x
    }
    fn h<'c>(&self, x: &'c i32, y: &'c i32) -> i32 {
        *x + *y + self.0
    }
}

struct T(i32);

impl Tr for T {
    fn f(&self, _x: &i32) -> &i32 {
        &self.0
    }
    fn g<'a>(&self, x: &'a i32, _y: &i32) -> &'a i32 {
        x
    }
    fn h(&self, _x: &i32, _y: &i32) -> i32 {
        0
    }
}

fn main() {}
