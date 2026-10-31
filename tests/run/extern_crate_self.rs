extern crate self as renamed;
pub fn value()->usize { 23 }
mod nested {
    extern crate self as local;
    pub fn get()->usize {local::value() + crate::renamed::value()}
}
fn main(){println!("{}", nested::get());}
