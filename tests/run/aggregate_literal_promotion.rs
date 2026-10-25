#[derive(Clone, Copy)]
enum Operand { Register, Memory(u16), Immediate(u8) }
struct Form { tag: u32, operands: &'static [Operand] }
const FORMS: &[Form] = &[
    Form { tag: 7, operands: &[Operand::Register, Operand::Memory(128)] },
    Form { tag: 11, operands: &[Operand::Immediate(9), Operand::Memory(256)] },
];
static STATIC_FORMS: &[Form] = &[
    Form { tag: 13, operands: &[Operand::Memory(512), Operand::Register] },
];
#[repr(align(64))]
struct Aligned { value: u32, fields: &'static [u16] }
struct Pair(u32, u16);
fn aligned() -> &'static Aligned { &Aligned { value: 17, fields: &[19, 23] } }
fn pair() -> &'static Pair { &Pair(29, 31) }
fn operand() -> &'static Operand { &Operand::Memory(1024) }
fn record_spread() -> &'static Pair { &Pair { 1: 41, ..Pair(37, 39) } }
fn score(operand: &Operand) -> u32 {
    match operand { Operand::Register => 1, Operand::Memory(bits) => *bits as u32, Operand::Immediate(value) => *value as u32 }
}
fn main() {
    let mut sum = 0;
    for form in FORMS { for op in form.operands { sum += form.tag + score(op); } }
    for form in STATIC_FORMS { for op in form.operands { sum += form.tag + score(op); } }
    let a = aligned();
    assert_eq!(a as *const Aligned as usize % 64, 0);
    assert_eq!(a.fields, &[19, 23]);
    assert_eq!(pair().1, 31);
    assert_eq!(score(operand()), 1024);
    assert_eq!(record_spread().0, 37);
    assert_eq!(record_spread().1, 41);
    println!("{} {} {} {} {} {}", sum, a.value, a.fields[1], pair().0, score(operand()), record_spread().1);
}
