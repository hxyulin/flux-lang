trait Value {}
impl Value for u32 {}
enum Access {
    Field(u32),
    Index(u32),
}

impl Access {
    fn field(&mut self, index: usize) -> Option<&mut dyn Value> {
        match self {
            Self::Field { 0: value } if index == 0 => Some(value),
            Self::Index { 0: value } if index == 0 && *value != 0 => Some(value),
            _ => None,
        }
    }
}

fn main() {
    assert!(Access::Field(4).field(0).is_some());
    assert!(Access::Index(4).field(0).is_some());
    assert!(Access::Index(0).field(0).is_none());
    assert!(Access::Index(4).field(1).is_none());
}
