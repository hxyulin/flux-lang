const NAME: &str = "flux";
static RAW: &str = r#"a "raw" string"#;

struct Entry<'a> {
    key: &'a str,
    value: u32,
}

fn first_char(s: &str) -> Option<char> {
    s.chars().next()
}

fn bytes(s: &str) -> &[u8] {
    s.as_bytes()
}

fn main() {
    let s = "héllo, wörld";
    let n = s.len();
    let empty = "".is_empty();
    let c = first_char(s);
    let b = bytes(NAME);
    let same = s == "héllo, wörld" && NAME < RAW;
    let starts = s.starts_with("hé") && s.ends_with('d');
    let entry = Entry { key: "k", value: 1 };
    let parsed = str::from_utf8(b"ok");
    let count = s.chars().rev().count() + s.bytes().count();
    let p: *const u8 = s.as_ptr();
}
