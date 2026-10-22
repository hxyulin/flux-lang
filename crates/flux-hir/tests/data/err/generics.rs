// Generic parameters and arguments rejected while lowering.

struct S<T> {
    a: T,
}

mod m {
    pub fn f() {}
}

// Duplicate parameters, including a method's shadowing its block's.
fn dup_fn<T, T>(x: T) {}

struct DupStruct<T, T> {
    a: T,
}

impl<T, T> S<T> {}

impl<T> S<T> {
    fn shadows_impl<T>(x: T) {}

    fn other() {}

    // Type arguments on `Self`.
    fn self_args() {
        let _ = Self::<i32>::other();
    }
}

// Type arguments where none are allowed.
fn on_local() {
    let x = 1;
    let _ = x::<u8>;
}

fn on_module() {
    m::<u8>::f();
}

fn on_param(s: S<i32>) {
    let _ = s::<u8>;
}

const C: i32 = 1;

fn on_const() {
    let _ = C::<u8>;
}

// Type parameters are types, not values, constructors or modules.
fn param_value<T>() {
    let _ = T;
}

fn param_literal<T>() {
    let _ = T { a: 1 };
}

fn param_assoc<T>() {
    T::new();
}

// Generic foreign functions.
unsafe extern "C" {
    fn foreign<T>();
}

// Inherent impl headers that name no nominal type.
impl<T> T {}

impl<T> *const T {}

impl<T> (T, T) {}

// Not supported yet.
fn lifetime<'a>(x: &'a i32) {}

fn const_param<const N: usize>() {}

fn bound<T: Copy>(x: T) {}

fn where_clause<T>(x: T)
where
    T: Copy,
{
}

fn main() {}
