#!/usr/bin/env python3
"""Generate compact Unicode debug-escape tables from pinned Rust core data.

Run: python3 scripts/generate_debug_unicode.py [--check] [--source unicode_data.rs]
The selected stable rustc exhaustively compares the result with char::escape_debug.
"""

import argparse
import hashlib
from pathlib import Path
import re
import subprocess
import tempfile

from generate_lowercase import SOURCE_SHA256

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "library/core/src/unicode.rs"

LOOKUP = """fn lookup(needle: u32, runs: &[u32], offsets: &[u8]) -> bool {
    let mut lo = 0;
    let mut hi = runs.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if (runs[mid] & ((1 << 21) - 1)) <= needle {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let mut at = (runs[lo] >> 21) as usize;
    let end = if lo + 1 < runs.len() { (runs[lo + 1] >> 21) as usize } else { offsets.len() };
    let previous = if lo == 0 { 0 } else { runs[lo - 1] & ((1 << 21) - 1) };
    let mut prefix = 0;
    let total = needle - previous;
    while at + 1 < end {
        prefix += offsets[at] as u32;
        if prefix > total { break; }
        at += 1;
    }
    at % 2 == 1
}

/// Rust's debug-printable categories, before its grapheme-extender test.
pub(crate) fn is_printable(c: char) -> bool {
    let point = c as u32;
    if point < 32 { return false; }
    if point < 127 { return true; }
    if point <= 0x9f || c.is_whitespace() { return false; }
    if matches!(point, 0xe000..=0xf8ff | 0xf0000..=0xffffd | 0x100000..=0x10fffd) {
        return false;
    }
    let assigned = match point {
        0..=0x377 => true,
        0x378..=0x3fffd => !lookup(point, &UNASSIGNED_RUNS, &UNASSIGNED_OFFSETS),
        0xe0001 | 0xe0020..=0xe007f | 0xe0100..=0xe01ef => true,
        _ => false,
    };
    assigned && !lookup(point, &FORMAT_RUNS, &FORMAT_OFFSETS)
        && !lookup(point, &IGNORABLE_RUNS, &IGNORABLE_OFFSETS)
}

/// Extenders escaped by debug formatting. Rust explicitly prints the two
/// halfwidth Katakana voicing marks literally despite their extender property.
pub(crate) fn is_grapheme_extended(c: char) -> bool {
    c > '\\u{2ff}' && !matches!(c, '\\u{ff9e}' | '\\u{ff9f}')
        && lookup(c as u32, &GRAPHEME_RUNS, &GRAPHEME_OFFSETS)
}
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--source", type=Path)
    args = parser.parse_args()
    source = args.source
    if source is None:
        sysroot = subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip()
        source = Path(sysroot) / "lib/rustlib/src/rust/library/core/src/unicode/unicode_data.rs"
    text = source.read_text()
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    if digest != SOURCE_SHA256:
        raise SystemExit(f"unexpected Unicode source hash {digest}; expected {SOURCE_SHA256}")
    output = [
        "//! Generated Unicode 18.0.0 debug-escape data. Regenerate with scripts/generate_debug_unicode.py.",
        "//! Derived from Rust core/src/unicode/unicode_data.rs (Apache-2.0 OR MIT).",
        f"//! Source SHA-256: {digest}", "", LOOKUP,
    ]
    for module, name in [("cn_planes_0_3", "UNASSIGNED"), ("cf", "FORMAT"),
                         ("default_ignorable_code_point", "IGNORABLE"), ("grapheme_extend", "GRAPHEME")]:
        start = text.index(f"pub mod {module} {{")
        runs_text = re.search(r"static SHORT_OFFSET_RUNS:[^=]+ = \[(.*?)\];", text[start:], re.S)[1]
        offsets_text = re.search(r"static OFFSETS:[^=]+ = \[(.*?)\];", text[start:], re.S)[1]
        runs = [(int(index) << 21) | int(prefix) for index, prefix in
                re.findall(r"ShortOffsetRunHeader::new\((\d+), (\d+)\)", runs_text)]
        offsets = list(map(int, re.findall(r"\d+", offsets_text)))
        assert runs and (runs[-1] & ((1 << 21) - 1)) > 0x10ffff
        assert all((run >> 21) < len(offsets) for run in runs)
        for suffix, values, ty in [("RUNS", runs, "u32"), ("OFFSETS", offsets, "u8")]:
            output.append(f"static {name}_{suffix}: [{ty}; {len(values)}] = [")
            output.extend("    " + ", ".join(map(str, values[i:i + 12])) + "," for i in range(0, len(values), 12))
            output.extend([ "];", ""])
    generated = "\n".join(output)
    with tempfile.TemporaryDirectory(prefix="flux-debug-unicode-") as directory:
        directory = Path(directory)
        candidate = directory / "unicode.rs"
        candidate.write_text(generated)
        driver = directory / "check.rs"
        driver.write_text('''pub use std::{ops, cmp};
mod intrinsics { pub const fn assume(value:bool) { assert!(value); } }
#[path=SOURCE] mod original;
#[path="unicode.rs"] mod data;
fn main() {
    assert_eq!(char::UNICODE_VERSION, (18, 0, 0));
    let mut checked = 0;
    for point in 0..=0x10ffff {
        let Some(c) = char::from_u32(point) else { continue; };
        let extending = point >= 0x300 && !matches!(point, 0xff9e | 0xff9f)
            && original::grapheme_extend::lookup(c);
        assert_eq!(data::is_grapheme_extended(c), extending, "U+{point:04X} grapheme");
        let expected = match c {
            '\\0' => "\\\\0".to_owned(), '\\t' => "\\\\t".to_owned(),
            '\\n' => "\\\\n".to_owned(), '\\r' => "\\\\r".to_owned(),
            '\\'' => "\\\\'".to_owned(), '"' => "\\\\\\\"".to_owned(), '\\\\' => "\\\\\\\\".to_owned(),
            _ if data::is_grapheme_extended(c) || !data::is_printable(c) => format!("\\\\u{{{:x}}}", point),
            _ => c.to_string(),
        };
        assert_eq!(expected, c.escape_debug().to_string(), "U+{point:04X}");
        checked += 1;
    }
    println!("verified debug escapes for {checked} Unicode scalars");
}
'''.replace("SOURCE", '"' + str(source.resolve()) + '"'))
        executable = directory / "check"
        subprocess.run(["rustc", "--edition=2024", "-Awarnings", "-O", str(driver), "-o", str(executable)], check=True)
        subprocess.run([str(executable)], check=True)
    if args.check:
        if OUTPUT.read_text() != generated:
            raise SystemExit(f"generated Unicode data differs: {OUTPUT}")
    else:
        OUTPUT.write_text(generated)


if __name__ == "__main__":
    main()
