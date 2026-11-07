trait Step {
    fn step(&self) -> i32;
}

struct A(i32);
struct B(i32);

impl Step for A {
    fn step(&self) -> i32 {
        self.0 + 10
    }
}

impl Step for B {
    fn step(&self) -> i32 {
        self.0 * 3
    }
}

#[inline(never)]
fn invoke<T: Step>(value: &T) -> i32 {
    value.step() + value.step()
}

#[inline(never)]
fn again(value: &A) -> i32 {
    invoke(value)
}

#[inline(never)]
fn erased(value: &dyn Step) -> i32 {
    value.step()
}

fn main() {
    let a = A(7);
    let b = B(5);
    let pointer: fn(&A) -> i32 = invoke::<A>;
    println!(
        "{} {} {} {} {}",
        invoke(&a),
        invoke(&b),
        again(&a),
        erased(&a),
        pointer(&a)
    );
}
