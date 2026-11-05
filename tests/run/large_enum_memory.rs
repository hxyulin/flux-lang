// Enums with more than 48 scalar leaves live in memory: copies, moves into
// aggregates, arguments, returns, matches and writes through references.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Wide {
    Empty,
    Small(u8),
    Pair(u64, i32),
    Many(u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64),
    Mixed(bool, u16, (u32, i64), [u8; 3]),
    Slice(&'static [u32], &'static str),
    Floats(f64, f32, f64, f32, f64, f32, f64, f32, f64, f32, f64, f32),
    More(i8, i16, i32, i64, u8, u16, u32, u64, i8, i16, i32, i64),
    Last(char, char, char, char, char, char, char, char, char, char, char, char),
}

fn sum(w: Wide) -> u64 {
    match w {
        Wide::Empty => 0,
        Wide::Small(a) => a as u64,
        Wide::Pair(a, b) => a + b as u64,
        Wide::Many(a, b, c, d, e, f, g, h, i, j, k, l) => a + b + c + d + e + f + g + h + i + j + k + l,
        Wide::Mixed(a, b, (c, d), e) => a as u64 + b as u64 + c as u64 + d as u64 + e[0] as u64 + e[2] as u64,
        Wide::Slice(s, t) => s.iter().map(|&x| x as u64).sum::<u64>() + t.len() as u64,
        Wide::Floats(a, _, c, _, _, _, _, _, _, _, _, l) => (a + c) as u64 + l as u64,
        Wide::More(a, _, _, d, _, _, _, h, _, _, _, l) => a as u64 + d as u64 + h + l as u64,
        Wide::Last(a, .., l) => a as u64 + l as u64,
    }
}

fn make(i: u64) -> Wide {
    match i % 9 {
        0 => Wide::Empty,
        1 => Wide::Small(i as u8),
        2 => Wide::Pair(i, 3),
        3 => Wide::Many(i, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11),
        4 => Wide::Mixed(true, 7, (i as u32, 9), [1, 2, 3]),
        5 => Wide::Slice(&[1, 2, 3], "four"),
        6 => Wide::Floats(1.5, 0.0, i as f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0),
        7 => Wide::More(1, 0, 0, i as i64, 0, 0, 0, 5, 0, 0, 0, 6),
        _ => Wide::Last('a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'z'),
    }
}

// Copy out of a reference into an enclosing aggregate, as `Option::copied`.
fn copied(w: Option<&(Wide, u8)>) -> Option<(Wide, u8)> {
    match w {
        Some(&pair) => Some(pair),
        None => None,
    }
}

fn first(pair: (Wide, Wide)) -> Wide {
    pair.0
}

fn bump(w: &mut Wide) {
    if let Wide::Many(a, .., l) = w {
        *a += 100;
        *l += 1;
    } else {
        *w = Wide::Pair(42, 1);
    }
}

fn main() {
    let mut total = 0;
    let mut all = Vec::new();
    for i in 0..18 {
        let w = make(i);
        let copy = w;
        total += sum(copy) + sum(w);
        all.push((w, i as u8));
    }
    println!("total {total}");
    let found = copied(all.get(3));
    println!("found {:?}", found);
    println!("missing {:?}", copied(all.get(99)));
    let array = [make(3), make(4), make(5)];
    let moved = [array[2], array[0]];
    println!("array {} {}", sum(moved[0]), sum(moved[1]));
    let picked = first((make(7), make(8)));
    println!("picked {:?}", picked);
    let mut edit = make(3);
    bump(&mut edit);
    let mut other = make(1);
    bump(&mut other);
    println!("edited {:?} {:?}", edit, other);
    let nested: Option<Option<Wide>> = Some(Some(make(6)));
    if let Some(Some(Wide::Floats(a, ..))) = nested {
        println!("nested {a}");
    }
    let same = all.iter().filter(|(w, _)| *w == make(3)).count();
    println!("same {same} {}", std::mem::size_of::<Wide>());
}
