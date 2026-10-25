mod debug {
    crate::entity!(DebugType, 41);
    crate::entity!(DebugScope, 47);
}
entity!(Root, 19);
self::factory!();
mod entity {
    #[macro_export]
    macro_rules! entity {
        ($name:ident, $value:literal) => {
            pub struct $name(u32);
            impl $name {
                pub fn new(extra: u32) -> Self { Self($value + extra + $crate::BASE) }
                pub fn value(&self) -> u32 { self.0 }
            }
        };
    }
}
mod factory {
    #[macro_export]
    macro_rules! factory {
        () => { pub fn generated() -> u32 { 73 } };
    }
}
const BASE: u32 = 3;
fn main() {
    let a = debug::DebugType::new(5);
    let b = debug::DebugScope::new(7);
    let c = Root::new(11);
    assert_eq!(a.value(), 49);
    assert_eq!(b.value(), 57);
    assert_eq!(c.value(), 33);
    println!("exported macros {} {} {} {}", a.value(), b.value(), c.value(), generated());
}
