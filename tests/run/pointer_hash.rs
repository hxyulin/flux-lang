//! Thin-pointer hashing goes through write_usize; compare addresses only
//! within each execution, so ASLR cannot change the observed output.
#[derive(Default)]
struct Recorded {
    address: usize,
    calls: usize,
}
impl std::hash::Hasher for Recorded {
    fn finish(&self) -> u64 {
        self.calls as u64
    }
    fn write(&mut self, _: &[u8]) {
        panic!("expected write_usize");
    }
    fn write_usize(&mut self, value: usize) {
        self.address = value;
        self.calls += 1;
    }
}
fn main() {
    let value = Box::new(17);
    let pointer: *const i32 = &*value;
    let mut h = Recorded::default();
    std::ptr::hash(pointer, &mut h);
    println!("thin {} {}", h.calls, h.address == pointer as usize);
    let mut raw_hash = Recorded::default();
    std::hash::Hash::hash(&pointer, &mut raw_hash);
    println!(
        "raw {}",
        raw_hash.address == h.address && raw_hash.calls == h.calls
    );
    let mutable = pointer as *mut i32;
    std::ptr::hash(mutable, &mut h);
    println!("mut {} {}", h.calls, h.address == pointer as usize);
    std::ptr::hash(core::ptr::null::<i32>(), &mut h);
    println!("null {} {}", h.calls, h.address == 0);

    // Clone a shared thin and fat reference to non-Clone pointees.
    struct NotClone(i32);
    let values = [NotClone(1), NotClone(2)];
    let reference = &values[0];
    let copy = Clone::clone(&reference);
    let slice: &[NotClone] = &values;
    let slice_copy = Clone::clone(&slice);
    let text = "héλ";
    let text_copy = Clone::clone(&text);
    println!(
        "references {} {} {} {}",
        core::ptr::eq(reference, copy),
        slice_copy.len(),
        slice_copy[1].0,
        text_copy
    );
}
