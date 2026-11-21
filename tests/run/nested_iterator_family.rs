trait LineNames<'a, S: 'static>: Iterator<Item = Self::Names<'a>> + ExactSizeIterator + Clone
where
    Self: 'a,
{
    type Names<'b>: Iterator<Item = &'b S> + ExactSizeIterator + Clone
    where
        Self: 'b;
}

trait Style {
    type Name: 'static;
    type Lines<'a>: LineNames<'a, Self::Name>
    where
        Self: 'a;

    fn lines(&self) -> Self::Lines<'_>;
}

type Lines<'a> =
    std::iter::Map<std::slice::Iter<'a, [u32; 2]>, fn(&[u32; 2]) -> std::slice::Iter<'_, u32>>;

impl<'a> LineNames<'a, u32> for Lines<'a> {
    type Names<'b>
        = std::slice::Iter<'b, u32>
    where
        Self: 'b;
}

struct Grid([[u32; 2]; 2]);

impl Style for Grid {
    type Name = u32;
    type Lines<'a> = Lines<'a>;

    fn lines(&self) -> Self::Lines<'_> {
        self.0.iter().map(|line| line.iter())
    }
}

fn count<S: 'static>(style: &impl Style<Name = S>) -> usize {
    let mut count = 0;
    for line in style.lines() {
        for _ in line.into_iter() {
            count += 1;
        }
    }
    count
}

fn main() {
    assert_eq!(count(&Grid([[1, 2], [3, 4]])), 4);
}
