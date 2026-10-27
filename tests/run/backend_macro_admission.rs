#[macro_export]
macro_rules! helper { () => { 17 }; }
#[macro_export(local_inner_macros)]
macro_rules! exported { () => { helper!() }; }
#[macro_export(local_inner_macros)]
macro_rules! captured { ($m:ident) => { $m!() }; }
#[macro_export(local_inner_macros)]
macro_rules! qualified { () => { core::stringify!(value) }; }
macro_rules! ordinary { () => { helper!() }; }
trait Marker {}
#[::core::prelude::v1::derive(::core::clone::Clone,::core::marker::Copy)]
struct Value;
#[diagnostic::do_not_recommend]
impl Marker for Value {}
fn require<T: Marker>(_: T) {}
macro_rules! emit_item {($i:item)=>{$i}}
macro_rules! declare {()=>{fn generated()->u32{31}}}
emit_item!(declare!(););
fn main() {
    macro_rules! helper { () => { 99 }; }
    assert_eq!(exported!(),17);
    assert_eq!(ordinary!(),99);
    assert_eq!(captured!(helper),99);
    assert_eq!(qualified!(),"value");
    require(Value.clone());assert_eq!(generated(),31);
    println!("macros {} {} {}",exported!(),ordinary!(),captured!(helper));
}
