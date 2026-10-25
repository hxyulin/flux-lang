#[derive(Clone, Copy)]
enum Type { I8, Wide(u16) }
const COUNT: usize = 2 + 1;
struct Signature { inputs: &'static [Type], outputs: &'static [Type] }
fn signature() -> Signature { Signature { inputs: &[Type::I8; COUNT], outputs: &[Type::Wide(128); 2] } }
struct DefaultArray<const N: usize = 3>([u16; N]);
fn repeated_default() -> &'static DefaultArray { &DefaultArray([513; COUNT]) }
struct LayoutInsts<'a> { insts: std::slice::Iter<'a, u32>, count: usize }
fn empty() -> LayoutInsts<'static> { LayoutInsts { insts: [].iter(), count: 0 } }
fn generic_empty<T>() -> std::slice::Iter<'static, T> { [].iter() }
fn empty_mut() -> std::slice::IterMut<'static, u32> { [].iter_mut() }
fn repeated() -> std::slice::Iter<'static, u16> { [513; COUNT].iter() }
#[derive(Clone, Copy)]
#[repr(align(64))]
struct Aligned(u32);
fn aligned() -> std::slice::Iter<'static, Aligned> { [Aligned(37); 2].iter() }
fn side(effects: &std::cell::Cell<usize>) -> u32 { effects.set(effects.get() + 1); 7 }
fn main() {
    let signature = signature();
    assert_eq!(signature.inputs.len(), 3);
    for value in signature.inputs { assert!(matches!(value, Type::I8)); }
    for value in signature.outputs { assert!(matches!(value, Type::Wide(128))); }
    assert_eq!(repeated_default().0, [513; 3]);
    let mut empty = empty();
    assert_eq!(empty.count, 0);
    assert_eq!(empty.insts.next(), None);
    assert_eq!(generic_empty::<String>().count(), 0);
    assert_eq!(empty_mut().count(), 0);
    assert_eq!(repeated().copied().sum::<u16>(), 1539);
    let mut aligned = aligned();
    let first = aligned.next().unwrap();
    assert_eq!(first as *const Aligned as usize % 64, 0);
    assert_eq!(first.0, 37);
    let effects = std::cell::Cell::new(0);
    assert_eq!([side(&effects); 0].iter().count(), 0);
    assert_eq!(effects.get(), 1);
    println!("{} {} {} {}", signature.inputs.len(), signature.outputs.len(), repeated_default().0[2], first.0);
}
