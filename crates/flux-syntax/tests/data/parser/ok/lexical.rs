/* block /* nested */ comment */
fn r#match() {
    let s = "escaped \" quote\n";
    let r = r##"raw "# still raw"##;
    let b = br"raw bytes";
    let c = '\'';
    let u = '\u{1F600}';
    let l: &'static str = "";
    let n = 1_000_000i64 + 0o17 + 0b1010 + 1.5e-3f64 as i64;
    let f = 1.max(2) + 1.0.max(2.0) as i32;
    let unicode = "héllo";
}
