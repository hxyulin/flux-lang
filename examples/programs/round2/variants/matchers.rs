//! flux variant of ../matchers.rs, minimally rewritten to compile with flux:
//! Atom::Class as a value becomes a closure; filter(|b| pred(b)) becomes a
//! for loop (works around bugs/closure_calls_captured_fn.rs);
//! u8::is_ascii_digit and is_ascii_whitespace as fn pointers become
//! closures (bugs/core_method_as_fn_value.rs).
//!
//! Pattern matching with backtracking over byte slices: a glob matcher
//! (`*`, `?`, `[a-z]`, `[!abc]`, `\` escapes) written iteratively with a
//! single backtrack point, and a small regex engine in the style of Pike
//! and Kernighan's, extended with `+`, `?`, character classes and
//! escapes, compiled first into a fixed array of nodes, then matched by
//! recursive backtracking. Both sit behind a `Matcher` trait with a
//! provided `find_all`, and a table-driven test checks each case.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

trait Matcher {
    /// Whether the pattern matches the whole text.
    fn is_match(&self, text: &[u8]) -> bool;

    /// Counts the texts that match, collecting their indices.
    fn find_all<'t>(&self, texts: &[&'t str], out: &mut [usize]) -> usize {
        let mut n = 0;
        for (i, t) in texts.iter().enumerate() {
            if self.is_match(t.as_bytes()) && n < out.len() {
                out[n] = i;
                n += 1;
            }
        }
        n
    }
}

struct Glob<'p> {
    pat: &'p [u8],
}

impl<'p> Glob<'p> {
    fn new(pat: &'p str) -> Self {
        Glob { pat: pat.as_bytes() }
    }

    /// Matches a `[...]` class at `p` against `c`; returns whether it matched
    /// and the index after the class.
    fn class(&self, mut p: usize, c: u8) -> (bool, usize) {
        p += 1;
        let negate = matches!(self.pat.get(p), Some(b'!' | b'^'));
        if negate {
            p += 1;
        }
        let mut matched = false;
        let mut first = true;
        while p < self.pat.len() && (first || self.pat[p] != b']') {
            first = false;
            let lo = self.pat[p];
            if self.pat.get(p + 1) == Some(&b'-') && self.pat.get(p + 2).is_some_and(|&b| b != b']') {
                let hi = self.pat[p + 2];
                matched |= (lo..=hi).contains(&c);
                p += 3;
            } else {
                matched |= lo == c;
                p += 1;
            }
        }
        (matched != negate, p + 1)
    }
}

impl Matcher for Glob<'_> {
    fn is_match(&self, text: &[u8]) -> bool {
        let (mut p, mut t) = (0, 0);
        let mut backtrack: Option<(usize, usize)> = None;
        while t < text.len() {
            let step = match self.pat.get(p) {
                Some(b'*') => {
                    backtrack = Some((p, t + 1));
                    p += 1;
                    continue;
                }
                Some(b'?') => Some(p + 1),
                Some(b'[') => match self.class(p, text[t]) {
                    (true, next) => Some(next),
                    (false, _) => None,
                },
                Some(b'\\') if self.pat.get(p + 1) == Some(&text[t]) => Some(p + 2),
                Some(&c) if c != b'\\' && c == text[t] => Some(p + 1),
                _ => None,
            };
            match (step, backtrack) {
                (Some(next), _) => {
                    p = next;
                    t += 1;
                }
                (None, Some((star, resume))) => {
                    p = star + 1;
                    t = resume;
                    backtrack = Some((star, resume + 1));
                }
                (None, None) => return false,
            }
        }
        self.pat[p..].iter().all(|&c| c == b'*')
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Atom {
    Any,
    Byte(u8),
    /// A set of bytes as a 256-bit bitmap.
    Class([u64; 4]),
}

impl Atom {
    fn matches(&self, c: u8) -> bool {
        match self {
            Atom::Any => true,
            Atom::Byte(b) => *b == c,
            Atom::Class(bits) => bits[(c >> 6) as usize] >> (c & 63) & 1 == 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Repeat {
    One,
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    atom: Atom,
    repeat: Repeat,
}

#[derive(Debug, PartialEq, Eq)]
enum RegexError {
    TooLong,
    DanglingRepeat(usize),
    UnclosedClass,
    TrailingBackslash,
}

const MAX_NODES: usize = 32;

struct Regex {
    nodes: [Node; MAX_NODES],
    len: usize,
    anchored_start: bool,
    anchored_end: bool,
}

fn set_bit(bits: &mut [u64; 4], c: u8) {
    bits[(c >> 6) as usize] |= 1 << (c & 63);
}

fn escape_class(c: u8) -> Option<[u64; 4]> {
    let mut bits = [0u64; 4];
    let pred: fn(&u8) -> bool = match c {
        b'd' => |b: &u8| b.is_ascii_digit(),
        b'w' => |b: &u8| b.is_ascii_alphanumeric() || *b == b'_',
        b's' => |b: &u8| b.is_ascii_whitespace(),
        _ => return None,
    };
    for b in 0..=255u8 {
        if pred(&b) {
            set_bit(&mut bits, b);
        }
    }
    Some(bits)
}

impl Regex {
    fn new(pattern: &str) -> Result<Regex, RegexError> {
        let mut p = pattern.as_bytes();
        let mut re = Regex {
            nodes: [Node { atom: Atom::Any, repeat: Repeat::One }; MAX_NODES],
            len: 0,
            anchored_start: false,
            anchored_end: false,
        };
        if let Some((&b'^', rest)) = p.split_first() {
            re.anchored_start = true;
            p = rest;
        }
        if let Some((&b'$', rest)) = p.split_last() {
            re.anchored_end = true;
            p = rest;
        }
        let mut i = 0;
        while i < p.len() {
            let atom = match p[i] {
                b'.' => Atom::Any,
                b'\\' => {
                    i += 1;
                    let c = *p.get(i).ok_or(RegexError::TrailingBackslash)?;
                    escape_class(c).map_or(Atom::Byte(c), |bits| Atom::Class(bits))
                }
                b'[' => {
                    let close = p[i + 1..].iter().position(|&b| b == b']').ok_or(RegexError::UnclosedClass)? + i + 1;
                    let body = &p[i + 1..close];
                    let (negate, body) = match body.split_first() {
                        Some((&b'^', rest)) => (true, rest),
                        _ => (false, body),
                    };
                    let mut bits = [0u64; 4];
                    let mut j = 0;
                    while j < body.len() {
                        if j + 2 < body.len() && body[j + 1] == b'-' {
                            for c in body[j]..=body[j + 2] {
                                set_bit(&mut bits, c);
                            }
                            j += 3;
                        } else {
                            set_bit(&mut bits, body[j]);
                            j += 1;
                        }
                    }
                    if negate {
                        bits.iter_mut().for_each(|w| *w = !*w);
                    }
                    i = close;
                    Atom::Class(bits)
                }
                b'*' | b'+' | b'?' => return Err(RegexError::DanglingRepeat(i)),
                c => Atom::Byte(c),
            };
            i += 1;
            let repeat = match p.get(i) {
                Some(b'*') => Repeat::ZeroOrMore,
                Some(b'+') => Repeat::OneOrMore,
                Some(b'?') => Repeat::ZeroOrOne,
                _ => Repeat::One,
            };
            if repeat != Repeat::One {
                i += 1;
            }
            if re.len == MAX_NODES {
                return Err(RegexError::TooLong);
            }
            re.nodes[re.len] = Node { atom, repeat };
            re.len += 1;
        }
        Ok(re)
    }

    fn match_here(&self, nodes: &[Node], text: &[u8]) -> Option<usize> {
        let Some((node, rest)) = nodes.split_first() else {
            return if !self.anchored_end || text.is_empty() { Some(0) } else { None };
        };
        let (min, max) = match node.repeat {
            Repeat::One => (1, 1),
            Repeat::ZeroOrOne => (0, 1),
            Repeat::ZeroOrMore => (0, usize::MAX),
            Repeat::OneOrMore => (1, usize::MAX),
        };
        // Greedy: take as many as possible, then back off.
        let available = text.iter().take(max).take_while(|&&c| node.atom.matches(c)).count();
        if available < min {
            return None;
        }
        (min..=available).rev().find_map(|n| self.match_here(rest, &text[n..]).map(|len| len + n))
    }

    /// The first match: its start and length.
    fn find(&self, text: &[u8]) -> Option<(usize, usize)> {
        let last_start = if self.anchored_start { 0 } else { text.len() };
        (0..=last_start).find_map(|start| {
            self.match_here(&self.nodes[..self.len], &text[start..]).map(|len| (start, len))
        })
    }
}

impl Matcher for Regex {
    fn is_match(&self, text: &[u8]) -> bool {
        self.find(text).is_some()
    }
}

fn show(s: &[u8]) {
    unsafe { printf(c"%.*s".as_ptr(), s.len() as i32, s.as_ptr()) };
}

fn main() {
    let globs: [(&str, &str, bool); 14] = [
        ("*.rs", "main.rs", true),
        ("*.rs", "main.rs.bak", false),
        ("src/*/mod.rs", "src/ast/mod.rs", true),
        ("a*b*c", "aXXbYYbZZc", true),
        ("a*b*c", "aXXbYYbZZ", false),
        ("???", "abc", true),
        ("???", "ab", false),
        ("[a-c]x[!0-9]", "bxz", true),
        ("[a-c]x[!0-9]", "bx7", false),
        ("*", "", true),
        ("\\*literal", "*literal", true),
        ("\\*literal", "xliteral", false),
        ("[]]", "]", true),
        ("*a*a*a*a*b", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", false),
    ];
    let mut failures = 0;
    for (pat, text, want) in globs {
        let got = Glob::new(pat).is_match(text.as_bytes());
        if got != want {
            failures += 1;
            show(b"GLOB MISMATCH: ");
            show(pat.as_bytes());
            show(b"\n");
        }
    }
    unsafe { printf(c"glob: %zu cases, %d failures\n".as_ptr(), globs.len(), failures) };

    let regexes: [(&str, &str, Option<(usize, usize)>); 14] = [
        ("abc", "xxabcxx", Some((2, 3))),
        ("^abc", "xxabc", None),
        ("^ab+c$", "abbbbc", Some((0, 6))),
        ("colou?r", "the color red", Some((4, 5))),
        ("colou?r", "the colour red", Some((4, 6))),
        ("a.*b", "xaxxbxxbx", Some((1, 7))),
        ("\\d+", "order 66 now", Some((6, 2))),
        ("[A-Z][a-z]*", "hello World", Some((6, 5))),
        ("[^aeiou ]+", "aeiou xyz", Some((6, 3))),
        ("\\w+@\\w+\\.com$", "mail me: ann@example.com", Some((9, 15))),
        ("x*", "", Some((0, 0))),
        ("^$", "", Some((0, 0))),
        ("a+a+a+a+b", "aaaaaaaaaaaaaaaaaaaa", None),
        ("\\s\\s", "one two  three", Some((7, 2))),
    ];
    let mut regex_failures = 0;
    for (pat, text, want) in regexes {
        let re = Regex::new(pat).unwrap();
        let got = re.find(text.as_bytes());
        if got != want {
            regex_failures += 1;
            show(b"REGEX MISMATCH: ");
            show(pat.as_bytes());
            show(b"\n");
        } else if let Some((s, l)) = got {
            show(pat.as_bytes());
            show(b" => '");
            show(&text.as_bytes()[s..s + l]);
            show(b"'\n");
        }
    }
    unsafe { printf(c"regex: %zu cases, %d failures\n".as_ptr(), regexes.len(), regex_failures) };

    let errors = [Regex::new("*a").err(), Regex::new("[abc").err(), Regex::new("ab\\").err(), Regex::new("a?b+").err()];
    let described: [&str; 4] = errors.map(|e| match e {
        Some(RegexError::DanglingRepeat(_)) => "dangling",
        Some(RegexError::UnclosedClass) => "unclosed",
        Some(RegexError::TrailingBackslash) => "backslash",
        Some(RegexError::TooLong) => "too long",
        None => "ok",
    });
    for d in described {
        show(d.as_bytes());
        show(b" ");
    }
    show(b"\n");

    let files = ["lib.rs", "main.rs", "Cargo.toml", "README.md", "build.rs", "notes.txt", "mod.rs"];
    let mut idx = [0usize; 8];
    let n = Glob::new("*.rs").find_all(&files, &mut idx);
    show(b"*.rs:");
    for &i in &idx[..n] {
        show(b" ");
        show(files[i].as_bytes());
    }
    let re = Regex::new("^[A-Z][A-Za-z]+\\.").unwrap();
    let m = re.find_all(&files, &mut idx);
    show(b"\ncapitalized:");
    for &i in &idx[..m] {
        show(b" ");
        show(files[i].as_bytes());
    }
    show(b"\n");
    unsafe { exit(failures + regex_failures) }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}
