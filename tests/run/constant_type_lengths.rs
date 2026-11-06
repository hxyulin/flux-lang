// Array lengths that name a constant whose own type has a computed length:
// keel-codegen's `[u64; Cover::ALL.len()]`, where `Cover::ALL` is
// `[Self; [$(stringify!($variant)),*].len()]`.

#[derive(Clone, Copy, Debug)]
enum Cover {
    Fold,
    Merge,
    Split,
}

impl Cover {
    const ALL: [Self; ["Fold", "Merge", "Split"].len()] = [Self::Fold, Self::Merge, Self::Split];
}

struct Profile {
    covers: [u64; Cover::ALL.len()],
}

struct Wide {
    lanes: [Cover; Cover::ALL.len() + 1],
}

impl Wide {
    const FIRST: Wide = Wide {
        lanes: [Cover::Fold; 4],
    };
}

struct Doubled {
    bytes: [u8; Wide::FIRST.lanes.len() * 2],
}

fn main() {
    let mut profile = Profile {
        covers: [0; Cover::ALL.len()],
    };
    for (i, cover) in Cover::ALL.into_iter().enumerate() {
        profile.covers[i] = cover as u64 + 10;
    }
    println!("{:?} {:?}", Cover::ALL, profile.covers);
    let doubled = Doubled { bytes: [7; 8] };
    println!("{:?} {}", Wide::FIRST.lanes, doubled.bytes.len());
}
