#![feature(specialization, box_patterns)]
trait Value { fn value(&self) -> u8; }
impl<T> Value for T {
    default fn value(&self) -> u8 { 0 }
}
default impl<T> Value for Option<T> {
    fn value(&self) -> u8 { 1 }
}
fn pattern(box x: Box<u8>) { let (box y, _) = (x, 0); }
fn default() { let default = 1; let r#default = default; }
macro_rules! make { () => { impl Value for u8 { default fn value(&self) -> u8 { 2 } } } }
