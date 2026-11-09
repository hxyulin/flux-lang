use std::ops::Index;

trait Storage { type Buffer; }
struct Block<G: Storage>(G::Buffer);

impl<G: Storage<Buffer = [u32; 4]>> Index<usize> for Block<G> {
    type Output = u32;
    fn index(&self, index: usize) -> &u32 { &self.0[index] }
}

struct Words;
impl Storage for Words { type Buffer = [u32; 4]; }

fn main() {
    let block = Block::<Words>([7, 11, 17, 23]);
    assert_eq!(block[2], 17);
    println!("{} {}", block[0], block[3]);
}
