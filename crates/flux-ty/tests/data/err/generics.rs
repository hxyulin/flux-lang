// Type errors involving generic parameters and instances.

struct Pair<T> {
    a: T,
    b: T,
}

struct Wrap<T>(T);

struct Plain {
    x: i32,
}

impl<T> Pair<T> {
    fn new(a: T, b: T) -> Self {
        Pair { a, b }
    }

    fn get(self) -> T {
        self.a
    }
}

impl Plain {
    fn plain(&self) -> i32 {
        self.x
    }
}

fn id<T>(x: T) -> T {
    x
}

fn two<A, B>(a: A, _b: B) -> A {
    a
}

// Wrong numbers of generic arguments.
fn too_many_on_fn() {
    let _ = id::<i32, u8>(1);
}

fn too_few_on_fn() {
    let _ = two::<i32>(1, 2);
}

fn too_many_on_struct(p: Pair<i32, u8>) {}

fn too_few_on_struct(p: Pair) {}

fn args_on_plain(p: Plain<i32>) {}

fn args_on_plain_method(p: Plain) {
    let _ = p.plain::<u8>();
}

fn too_many_on_literal() {
    let _ = Pair::<i32, u8> { a: 1, b: 2 };
}

fn too_many_on_tuple_struct() {
    let _ = Wrap::<i32, u8>(1);
}

fn args_on_primitive(x: i32<u8>) {}

fn args_on_param<T>(x: T<u8>) {}

// Parameters that are never used or constrained.
struct Unused<T> {
    x: i32,
}

struct Unit<T>;

impl<T> Plain {}

// Nothing determines the parameter.
fn diverge<T>() -> T {
    loop {}
}

fn not_inferred() {
    let x = diverge();
}

fn not_inferred_statement() {
    diverge();
}

fn placeholder_turbofish() {
    let _ = diverge::<_>();
}

fn placeholder_not_inferred() {
    let p: Pair<_>;
}

// Mismatches between parameters, concrete types and instances.
fn param_vs_concrete<T>(x: T) -> i32 {
    x
}

fn concrete_vs_param<T>() -> T {
    1
}

fn param_vs_param<T, U>(x: T) -> U {
    x
}

fn instance_mismatch(p: Pair<i32>) -> Pair<u8> {
    p
}

fn arg_mismatch() {
    let _ = Pair::new(1i32, 2u8);
}

fn turbofish_mismatch() {
    let _ = id::<u8>(true);
}

fn annotation_mismatch() {
    let _: Pair<bool> = Pair::new(true, 2);
}

fn field_mismatch() {
    let _ = Pair::<u8> { a: 1, b: false };
}

fn wrap_mismatch(w: Wrap<Wrap<i32>>) -> Wrap<i32> {
    w
}

// A parameter has no operators, fields or methods.
fn add<T>(a: T, b: T) -> T {
    a + b
}

fn compare<T>(a: T, b: T) -> bool {
    a == b
}

fn negate<T>(a: T) -> T {
    -a
}

fn cast<T>(a: T) -> i32 {
    a as i32
}

fn field<T>(a: T) -> i32 {
    a.x
}

fn method<T>(a: T) {
    a.go();
}

fn deref<T>(a: T) {
    let _ = *a;
}

fn call<T>(a: T) {
    a();
}

// Several `impl` blocks: a method of one instance is not one of another.
impl Pair<i32> {
    fn total(&self) -> i32 {
        self.a + self.b
    }
}

impl Pair<u8> {
    fn total(&self) -> u8 {
        self.a + self.b
    }

    fn only_u8(&self) {}
}

fn ambiguous_instance() {
    let p = Pair::new(1, 2);
    let _ = p.total();
}

fn wrong_instance(p: Pair<bool>) {
    p.only_u8();
}

fn no_total(p: Pair<bool>) {
    p.total();
}

// Overlapping blocks define the same name twice.
impl<T> Pair<T> {
    fn dup(&self) {}
}

impl Pair<i32> {
    fn dup(&self) {}
}

// Privacy of generic structs, their fields and methods.
mod m {
    struct Hidden<T> {
        a: T,
    }

    pub struct Open<T> {
        pub a: T,
        b: T,
    }

    pub fn open() -> Open<i32> {
        Open { a: 1, b: 2 }
    }

    pub fn nested() -> Open<Open<u8>> {
        Open {
            a: Open { a: 1, b: 2 },
            b: Open { a: 3, b: 4 },
        }
    }

    impl<T> Open<T> {
        fn secret(self) -> T {
            self.b
        }
    }
}

fn private_struct(h: m::Hidden<i32>) {}

fn private_field() -> i32 {
    m::open().b
}

fn private_nested_field() -> u8 {
    m::nested().a.b
}

fn private_method() -> i32 {
    m::open().secret()
}

// An assignment's place gives calls and struct expressions their type.
fn assign_call(mut x: i32) {
    x = id(1u8);
    let _ = x;
}

fn assign_literal(mut p: Pair<i32>) {
    p = Pair { a: 1u8, b: 2 };
    let _ = p;
}

// Not supported yet: references as generic arguments.
fn reference_argument() {
    let x = 1;
    let _ = id(&x);
}

fn reference_in_instance(p: Pair<&i32>) {}

fn reference_in_literal() {
    let x = 1;
    let _ = Pair { a: &x, b: &x };
}

fn main() {}
