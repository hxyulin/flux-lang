trait Words: Iterator<Item = u32> {}
impl<I: Iterator<Item = u32>> Words for I {}
fn sum_words(words: &mut dyn Words) -> u32 { words.sum() }
fn main() { let mut words = [3u32, 5, 7].into_iter(); println!("{}", sum_words(&mut words)); }
