#[non_exhaustive]
struct Named { value: i32 }
#[non_exhaustive]
struct Tuple(i32);
#[non_exhaustive]
struct Unit;
#[non_exhaustive]
enum Choice { A, B(i32), C { value: i32 } }
#[non_exhaustive]
enum Empty {}
fn empty(x: Empty) -> i32 { match x {} }
fn inspect(value: i32) -> i32 {
    let base = Named { value };
    let Named { value } = Named { ..base };
    let Tuple(x) = Tuple(value + 1);
    let Unit = Unit;
    let choices = [Choice::A, Choice::B(x), Choice::C { value }];
    let mut total = 0;
    for c in choices {
        total += match c { Choice::A => 10, Choice::B(x) => x, Choice::C { value } => value };
    }
    total
}
fn main() { println!("{} {}", inspect(7), inspect(19)); }
