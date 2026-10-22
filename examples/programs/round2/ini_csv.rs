//! Two line-oriented text formats parsed without allocation, borrowing the
//! input throughout: an INI parser as an iterator of `Entry<'a>` items
//! (sections, key/value pairs, with comments and continuation errors), and
//! a CSV reader with quoted fields (`"a, b"`, doubled quotes) yielding
//! `&'a str` fields per record. Numbers are parsed by hand into a custom
//! error type, and errors convert between layers with `From` and `?`.
//! Configuration values are typed through a `FromValue` trait.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumError {
    Empty,
    InvalidDigit(u8),
    Overflow,
}

fn parse_i64(s: &str) -> Result<i64, NumError> {
    let (neg, digits) = match s.as_bytes() {
        [] => return Err(NumError::Empty),
        [b'-', rest @ ..] => (true, rest),
        [b'+', rest @ ..] => (false, rest),
        all => (false, all),
    };
    if digits.is_empty() {
        return Err(NumError::Empty);
    }
    let mut value: i64 = 0;
    for &d in digits {
        if d == b'_' {
            continue;
        }
        let digit = (d as char).to_digit(10).ok_or(NumError::InvalidDigit(d))? as i64;
        value = value
            .checked_mul(10)
            .and_then(|v| if neg { v.checked_sub(digit) } else { v.checked_add(digit) })
            .ok_or(NumError::Overflow)?;
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigError<'a> {
    Syntax { line: usize, text: &'a str },
    Missing { section: &'a str, key: &'a str },
    BadValue { key: &'a str, cause: ValueError },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueError {
    Number(NumError),
    Bool,
    Range { min: i64, max: i64 },
}

impl From<NumError> for ValueError {
    fn from(e: NumError) -> Self {
        ValueError::Number(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry<'a> {
    Section(&'a str),
    Pair { section: &'a str, key: &'a str, value: &'a str },
}

struct Ini<'a> {
    lines: core::iter::Enumerate<core::str::Lines<'a>>,
    section: &'a str,
}

impl<'a> Ini<'a> {
    fn new(text: &'a str) -> Self {
        Ini { lines: text.lines().enumerate(), section: "" }
    }
}

impl<'a> Iterator for Ini<'a> {
    type Item = Result<Entry<'a>, ConfigError<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (n, raw) = self.lines.next()?;
            let line = match raw.find(|c| c == ';' || c == '#') {
                Some(i) => &raw[..i],
                None => raw,
            }
            .trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                return Some(match rest.strip_suffix(']') {
                    Some(name) if !name.trim().is_empty() => {
                        self.section = name.trim();
                        Ok(Entry::Section(self.section))
                    }
                    _ => Err(ConfigError::Syntax { line: n + 1, text: raw }),
                });
            }
            return Some(match line.split_once('=') {
                Some((k, v)) if !k.trim().is_empty() => {
                    let v = v.trim();
                    let v = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(v);
                    Ok(Entry::Pair { section: self.section, key: k.trim(), value: v })
                }
                _ => Err(ConfigError::Syntax { line: n + 1, text: raw }),
            });
        }
    }
}

trait FromValue: Sized {
    fn from_value(s: &str) -> Result<Self, ValueError>;
}

impl FromValue for i64 {
    fn from_value(s: &str) -> Result<Self, ValueError> {
        Ok(parse_i64(s)?)
    }
}

impl FromValue for bool {
    fn from_value(s: &str) -> Result<Self, ValueError> {
        match s {
            "true" | "yes" | "on" | "1" => Ok(true),
            "false" | "no" | "off" | "0" => Ok(false),
            _ => Err(ValueError::Bool),
        }
    }
}

impl<'s> FromValue for &'s str {
    fn from_value(_: &str) -> Result<Self, ValueError> {
        Err(ValueError::Bool)
    }
}

fn get_raw<'a>(text: &'a str, section: &'a str, key: &'a str) -> Result<&'a str, ConfigError<'a>> {
    for entry in Ini::new(text) {
        if let Entry::Pair { section: s, key: k, value } = entry? {
            if s == section && k == key {
                return Ok(value);
            }
        }
    }
    Err(ConfigError::Missing { section, key })
}

fn get<'a, T: FromValue>(text: &'a str, section: &'a str, key: &'a str) -> Result<T, ConfigError<'a>> {
    let raw = get_raw(text, section, key)?;
    T::from_value(raw).map_err(|cause| ConfigError::BadValue { key, cause })
}

fn get_in_range<'a>(text: &'a str, section: &'a str, key: &'a str, min: i64, max: i64) -> Result<i64, ConfigError<'a>> {
    let v: i64 = get(text, section, key)?;
    if (min..=max).contains(&v) {
        Ok(v)
    } else {
        Err(ConfigError::BadValue { key, cause: ValueError::Range { min, max } })
    }
}

/// Splits one CSV record into fields, honoring quotes. Quoted fields are
/// returned without their outer quotes (doubled quotes stay doubled).
struct CsvFields<'a> {
    rest: Option<&'a str>,
}

impl<'a> Iterator for CsvFields<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<&'a str> {
        let s = self.rest?;
        if let Some(body) = s.strip_prefix('"') {
            let bytes = body.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'"' {
                    if bytes.get(i + 1) == Some(&b'"') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            let field = &body[..i];
            let after = &body[(i + 1).min(body.len())..];
            self.rest = after.strip_prefix(',');
            Some(field)
        } else {
            match s.split_once(',') {
                Some((field, rest)) => {
                    self.rest = Some(rest);
                    Some(field)
                }
                None => {
                    self.rest = None;
                    Some(s)
                }
            }
        }
    }
}

fn fields(record: &str) -> CsvFields<'_> {
    CsvFields { rest: Some(record) }
}

const CONFIG: &str = "
; server configuration
[server]
host = example.org
port = 8080
workers = 16   # threads
tls = yes

[limits]
max_body = 1_048_576
timeout = -30
retries = 99999999999999999999
name = \"quoted value\"

[broken
";

const CSV: &str = "city,country,population,area
Tokyo,Japan,37400068,2191
\"Delhi, NCT\",India,28514000,1484
Shanghai,China,25582000,6341
\"São Paulo\",Brazil,21650000,1521
\"The \"\"Big\"\" Apple\",USA,18819000,783
Cairo,Egypt,20076000,3085";

fn print_str(s: &str) {
    unsafe { printf(c"%.*s".as_ptr(), s.len() as i32, s.as_ptr()) };
}

fn report(err: &ConfigError<'_>) {
    match *err {
        ConfigError::Syntax { line, text } => {
            unsafe { printf(c"syntax error on line %zu: ".as_ptr(), line) };
            print_str(text.trim());
        }
        ConfigError::Missing { section, key } => {
            print_str("missing ");
            print_str(section);
            print_str(".");
            print_str(key);
        }
        ConfigError::BadValue { key, cause } => {
            print_str("bad value for ");
            print_str(key);
            match cause {
                ValueError::Number(NumError::Overflow) => print_str(": overflow"),
                ValueError::Number(NumError::InvalidDigit(d)) => unsafe { printf(c": invalid digit '%c'".as_ptr(), d as i32); },
                ValueError::Number(NumError::Empty) => print_str(": empty"),
                ValueError::Bool => print_str(": not a bool"),
                ValueError::Range { min, max } => unsafe { printf(c": not in %lld..=%lld".as_ptr(), min, max); },
            }
        }
    }
    print_str("\n");
}

fn main() {
    let mut sections = 0;
    let mut pairs = 0;
    for entry in Ini::new(CONFIG) {
        match entry {
            Ok(Entry::Section(name)) => {
                sections += 1;
                print_str("[");
                print_str(name);
                print_str("]\n");
            }
            Ok(Entry::Pair { key, value, .. }) => {
                pairs += 1;
                print_str("  ");
                print_str(key);
                print_str(" = ");
                print_str(value);
                print_str("\n");
            }
            Err(e) => report(&e),
        }
    }
    unsafe { printf(c"%d sections, %d pairs\n".as_ptr(), sections, pairs) };

    let port = get_in_range(CONFIG, "server", "port", 1, 65535);
    let workers: Result<i64, _> = get(CONFIG, "server", "workers");
    let tls: Result<bool, _> = get(CONFIG, "server", "tls");
    let body = get::<i64>(CONFIG, "limits", "max_body");
    if let (Ok(p), Ok(w), Ok(t), Ok(b)) = (port, workers, tls, body) {
        unsafe { printf(c"port %lld workers %lld tls %d max_body %lld\n".as_ptr(), p, w, t as i32, b) };
    }
    let failures = [
        get_in_range(CONFIG, "limits", "timeout", 0, 3600).err(),
        get::<i64>(CONFIG, "limits", "retries").err(),
        get::<i64>(CONFIG, "server", "host").err(),
        get::<bool>(CONFIG, "server", "workers").err(),
        get::<i64>(CONFIG, "client", "port").err(),
    ];
    for e in failures.iter().flatten() {
        report(e);
    }

    // CSV: header, then records; compute totals and the densest city.
    let mut lines = CSV.lines();
    let header: [&str; 4] = {
        let mut h = [""; 4];
        for (slot, f) in h.iter_mut().zip(fields(lines.next().unwrap())) {
            *slot = f;
        }
        h
    };
    let pop_col = header.iter().position(|&h| h == "population").unwrap();
    let area_col = header.iter().position(|&h| h == "area").unwrap();
    let mut total_pop: i64 = 0;
    let mut densest: (&str, i64) = ("", 0);
    let mut rows = 0;
    for line in lines {
        let mut cols = [""; 4];
        let n = cols.iter_mut().zip(fields(line)).map(|(slot, f)| *slot = f).count();
        assert_eq!(n, 4, "short record");
        let pop = parse_i64(cols[pop_col]).unwrap();
        let area = parse_i64(cols[area_col]).unwrap();
        total_pop += pop;
        let density = pop / area;
        if density > densest.1 {
            densest = (cols[0], density);
        }
        rows += 1;
        print_str(cols[0]);
        print_str(" | ");
        print_str(cols[1]);
        unsafe { printf(c" | %lld/km2\n".as_ptr(), density) };
    }
    unsafe { printf(c"%d rows, total population %lld, densest: ".as_ptr(), rows, total_pop) };
    print_str(densest.0);
    print_str("\n");

    let bad_numbers = ["", "-", "12a", "9223372036854775807", "-9223372036854775808", "9223372036854775808"];
    for s in bad_numbers {
        match parse_i64(s) {
            Ok(v) => unsafe { printf(c"ok %lld\n".as_ptr(), v); },
            Err(e) => unsafe { printf(c"err %d\n".as_ptr(), matches!(e, NumError::Overflow) as i32); },
        }
    }
}
