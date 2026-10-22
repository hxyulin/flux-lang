//! `str`'s searching methods with each kind of pattern: a `&str` (empty
//! too), a `char`, a closure, arrays and slices of `char`s: `find`,
//! `rfind`, `strip_prefix`, `strip_suffix`, `split_once`, `rsplit_once`,
//! `trim_matches` and its one-sided forms, `split` and `rsplit` (from
//! both ends), `split_terminator`, `split_inclusive`, `splitn`,
//! `rsplitn`, `matches` and `match_indices`; and the sizes and back ends
//! of `bytes`, `chars` and `char_indices`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

fn text(label: i32, s: &str) {
    unsafe {
        printf(c"%d: [%.*s]\n".as_ptr(), label, s.len() as i32, s.as_ptr());
    }
}

fn opt(label: i32, s: Option<&str>) {
    match s {
        Some(s) => text(label, s),
        None => show(label, -1),
    }
}

fn pair(label: i32, p: Option<(&str, &str)>) {
    match p {
        Some((a, b)) => {
            text(label, a);
            text(label, b);
        }
        None => show(label, -1),
    }
}

fn index(label: i32, i: Option<usize>) {
    show(label, i.map_or(-1, |i| i as i64));
}

fn hint(label: i32, h: (usize, Option<usize>)) {
    let upper = match h.1 {
        Some(n) => n as i64,
        None => -1,
    };
    unsafe {
        printf(c"%d: hint %llu %lld\n".as_ptr(), label, h.0 as u64, upper);
    }
}

fn parts<'a, I: Iterator<Item = &'a str>>(label: i32, iter: I) {
    let mut n = 0;
    for part in iter {
        text(label, part);
        n += 1;
    }
    show(label, n);
}

fn main() {
    let line = "[section] ; comment # more";
    // find and rfind.
    index(1, line.find(';'));
    index(2, line.find(|c: char| c == ';' || c == '#'));
    index(3, line.find(|c| c == '#'));
    index(4, line.find(['#', ';']));
    index(5, line.find(&['#', 'c']));
    index(6, line.find(&[' ', ';'][..]));
    index(7, line.rfind(' '));
    index(8, line.rfind("me"));
    index(9, line.rfind(|c: char| c.is_ascii_uppercase()));
    index(10, line.find(""));
    index(11, line.rfind(""));
    index(12, "aaa".rfind("aa"));
    index(13, "héllo wörld".rfind('ö'));
    index(14, "héllo".find(&"llo"));
    // strip_prefix and strip_suffix.
    opt(20, line.strip_prefix('['));
    opt(21, line.strip_prefix("[sec"));
    opt(22, line.strip_prefix("sec"));
    opt(23, "a.rs".strip_suffix(".rs"));
    opt(24, "a.rs".strip_suffix(|c: char| c == 's'));
    opt(25, "xyz".strip_prefix(""));
    opt(26, "éa".strip_prefix(['é', 'a']));
    // split_once and rsplit_once.
    pair(30, "key=value=more".split_once('='));
    pair(31, "key=value=more".rsplit_once('='));
    pair(32, "key = value".split_once(" = "));
    pair(33, "novalue".split_once('='));
    pair(34, "ab".rsplit_once(""));
    pair(35, "ab".split_once(""));
    // trim_matches and its one-sided forms.
    text(40, "xxhixx".trim_matches('x'));
    text(41, "123abc456".trim_matches(|c: char| c.is_ascii_digit()));
    text(42, "--==hi==--".trim_start_matches(['-', '=']));
    text(43, "--==hi==--".trim_end_matches(&['-', '='][..]));
    text(44, "ababxab".trim_start_matches("ab"));
    text(45, "abxabab".trim_end_matches("ab"));
    text(46, "xxxx".trim_matches('x'));
    text(47, "abc".trim_start_matches(""));
    // split, from both ends.
    parts(50, "a,b,,c,".split(','));
    parts(51, "a,b,,c,".split(',').rev());
    parts(52, "ab".split(""));
    parts(53, "a--b----c".split("--"));
    parts(54, "a1b22c".split(|c: char| c.is_ascii_digit()));
    parts(55, "a b\tc".rsplit([' ', '\t']));
    parts(56, "aaa".rsplit("aa"));
    parts(57, "ab".rsplit(""));
    let mut both = "1,2,3,4".split(',');
    text(58, both.next().unwrap_or("?"));
    text(58, both.next_back().unwrap_or("?"));
    parts(58, both);
    // split_terminator, split_inclusive, splitn and rsplitn.
    parts(60, "A.B.".split_terminator('.'));
    parts(61, "A..B..".split_terminator('.'));
    parts(62, "".split_terminator('.'));
    parts(63, "A.B.".split_terminator('.').rev());
    parts(64, "line1\nline2\nline3\n".split_inclusive('\n'));
    parts(65, "a\nb".split_inclusive('\n'));
    parts(66, "a\nb\n".split_inclusive('\n').rev());
    parts(67, "a,b,c,d".splitn(2, ','));
    parts(68, "a,b,c,d".rsplitn(3, ','));
    parts(69, "a,b".splitn(0, ','));
    parts(70, "a b c".splitn(5, ' '));
    // matches and match_indices.
    parts(80, "abcXXabcYYabc".matches("abc"));
    parts(81, "1a2b33".matches(|c: char| c.is_ascii_digit()));
    parts(82, "1a2b33".rmatches(|c: char| c.is_ascii_digit()));
    for (at, m) in "xaxbxc".match_indices('x') {
        show(83, at as i64);
        text(83, m);
    }
    for (at, m) in "aaaa".rmatch_indices("aa") {
        show(84, at as i64);
        text(84, m);
    }
    show(85, "ab".matches("").count() as i64);
    // starts_with, ends_with and contains with the new patterns.
    show(90, line.starts_with(|c: char| c == '[') as i64);
    show(91, line.ends_with(['e', 'x']) as i64);
    show(92, line.contains(&['#'][..]) as i64);
    show(93, "".starts_with(|_: char| true) as i64);
    // bytes, chars and char_indices.
    let s = "héllo";
    hint(100, s.bytes().size_hint());
    show(101, s.bytes().len() as i64);
    index(102, s.bytes().rposition(|b| b == b'l'));
    hint(103, s.chars().size_hint());
    hint(104, s.char_indices().size_hint());
    for (at, c) in s.char_indices().rev() {
        show(105, at as i64);
        show(105, c as i64);
    }
    let mut chars = s.chars();
    chars.next();
    text(106, chars.as_str());
    let mut indices = s.char_indices();
    indices.next();
    indices.next();
    show(107, indices.offset() as i64);
    text(107, indices.as_str());
    show(108, s.bytes().rev().map(|b| b as i64).sum::<i64>());
}
