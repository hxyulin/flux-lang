use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::Location;

fn main() {
    let location = Location::caller();
    let mut actual = DefaultHasher::new();
    Hash::hash(&location, &mut actual);

    let mut fields = DefaultHasher::new();
    location.file().hash(&mut fields);
    location.line().hash(&mut fields);
    location.column().hash(&mut fields);
    assert_eq!(actual.finish(), fields.finish());
}
