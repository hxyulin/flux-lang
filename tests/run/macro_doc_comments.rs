macro_rules! define { ($token:literal pub struct $name:ident/$len:tt #[doc=$usage:literal]) => { struct $name; impl $name {fn value()->usize{$len}} }; }
define! { "_" pub struct Underscore/7 /// wildcard
}
fn main(){println!("{}",Underscore::value());}
