// A derived Default whose field type is unresolved reports that type
// alone, not the trait call of the derived body.
#[derive(Default)]
struct Cache {
    libraries: Vec<Missing>,
}

fn main() {}
