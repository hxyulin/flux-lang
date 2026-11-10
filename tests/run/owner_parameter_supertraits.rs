trait Machine {
    type Word;
}
trait Lane<W> {
    fn word(self) -> W;
}
trait Vector<M: Machine>: Lane<M::Word> {}
struct Cpu;
impl Machine for Cpu {
    type Word = u32;
}
struct Value(u32);
impl Lane<u32> for Value {
    fn word(self) -> u32 {
        self.0
    }
}
impl Vector<Cpu> for Value {}
fn word<M: Machine, V: Vector<M>>(value: V) -> M::Word {
    value.word()
}
fn main() {
    assert_eq!(word::<Cpu, _>(Value(17)), 17);
    println!("owner parameter supertraits passed");
}
