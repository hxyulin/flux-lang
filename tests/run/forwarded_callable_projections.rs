fn visit(mut f: impl FnMut(u32)) {
    let values = [1u32, 3, 7];
    values.iter().copied().for_each(&mut f);
    values.into_iter().for_each(f);
}
fn visit_mut(values: &mut [u32], mut f: impl FnMut(&mut u32)) {
    values.iter_mut().for_each(&mut f);
    values.iter_mut().for_each(f);
}
struct Entry { value: u32 }
struct Owner { entries: Vec<Entry> }
impl Owner {
    fn entry_mut(&mut self, index: usize) -> &mut Entry { &mut self.entries[index] }
}
fn main() {
    let mut sum = 0;
    let mut calls = 0;
    visit(|value| { sum += value; calls += 1; });
    assert_eq!(sum, 22);
    assert_eq!(calls, 6);
    let mut values = [1u32, 3, 7];
    visit_mut(&mut values, |value| { *value += 5; calls += 1; });
    assert_eq!(values, [11, 13, 17]);
    assert_eq!(calls, 12);
    let mut owners = vec![Owner { entries: vec![Entry { value: 3 }] }, Owner { entries: vec![Entry { value: 7 }] }];
    let updates: Vec<_> = owners.iter().enumerate().map(|(i, owner)| (i, 0usize, owner.entries[0].value + 29)).collect();
    for (i, entry, value) in updates { owners[i].entry_mut(entry).value = value; }
    assert_eq!(owners[0].entries[0].value, 32);
    assert_eq!(owners[1].entries[0].value, 36);
    println!("forward projections {} {} {} {}", sum, calls, owners[0].entries[0].value, owners[1].entries[0].value);
}
