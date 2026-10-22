// Enabled unions remain unsupported. Removed declarations must not resolve
// their fields, expand their derives, or shadow identifiers named `union`.
#[cfg(any())]
#[derive(Copy, Clone)]
union Removed {
    fallback: MissingType,
    #[cfg(target_arch = "aarch64")]
    neon: MissingNeon,
}

macro_rules! discard_item {
    ($item:item) => {};
}
discard_item! {
    union Matched { small: u8, large: u64 }
}

macro_rules! item {
    ($item:item) => { $item };
}
item! {
    #[cfg(any())]
    union Expanded { word: u64 }
}

macro_rules! union {
    () => { 7 };
}

fn union(union: u32) -> u32 {
    let r#union = union + 1;
    r#union
}

fn main() {
    #[cfg(any())]
    union Local { value: MissingLocal }
    let union = union(34);
    println!("{} {}", union, union!());
}
