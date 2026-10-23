struct Expr<'a> { value: &'a i32 }
struct Nested<T> { value: T }
type Alias<'a> = Nested<Expr<'a>>;
type Static = &'static i32;

fn identity<T>(value: T) -> T { value }
fn nth_child<T>(value: T) -> T { value }
fn split_at<T>(value: T) -> T { value }

impl<'t> Expr<'t> {
    fn next(self) -> Self { nth_child::<Expr<'t>>(self) }
    fn split(self) -> Self { split_at::<Expr<'t>>(self) }
    fn via<T>(self, value: T) -> T { identity::<T>(value) }
}

fn exercise<'a, 'b: 'a>(x: &'a i32, y: &'b i32) -> i32 {
    let first = nth_child::<Expr<'a>>(Expr { value: x });
    let second = split_at::<Expr<'a>>(Expr { value: y });
    let alias = identity::<Alias<'a>>(Nested { value: first.next().split() });
    let nested = identity::<Nested<(Expr<'a>, [&'b i32; 1])>>(Nested {
        value: (second, [y]),
    });
    let elided = identity::<Expr<'_>>(Expr { value: x });
    let omitted = identity::<Expr>(Expr { value: y });
    let inferred = identity::<_>(Expr { value: x });
    let static_ref = identity::<Static>(&7);
    let static_named = identity::<&'static i32>(&11);
    let method = inferred.via::<&'a i32>(x);
    *alias.value.value + *nested.value.0.value + *nested.value.1[0]
        + *elided.value + *omitted.value + *method + *static_ref + *static_named
}

fn main() {
    println!("{}", exercise(&3, &5));
}
