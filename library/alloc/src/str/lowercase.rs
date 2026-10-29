//! Unicode default lowercase, including the language-independent Final_Sigma rule.
//! Mapping/property facts derive from Rust's Unicode 17.0.0 tables; the sigma
//! context follows Unicode's default casing algorithm, as Rust alloc/str.rs does.
use crate::string::String;

#[path = "lowercase_data.rs"]
mod data;

fn in_ranges(point: u32, ranges: &[(u32, u32)]) -> bool {
    let mut low = 0;
    let mut high = ranges.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if ranges[mid].1 < point {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low < ranges.len() && ranges[low].0 <= point
}
pub(super) fn is_cased(value: char) -> bool {
    in_ranges(value as u32, &data::CASED)
}
pub(super) fn is_case_ignorable(value: char) -> bool {
    in_ranges(value as u32, &data::CASE_IGNORABLE)
}

fn final_sigma(input: &str, index: usize) -> bool {
    let mut before = input[..index].chars();
    let mut preceded = false;
    while let Some(value) = before.next_back() {
        if !is_case_ignorable(value) {
            preceded = is_cased(value);
            break;
        }
    }
    if !preceded {
        return false;
    }
    let after = input[index + 'Σ'.len_utf8()..].chars();
    for value in after {
        if !is_case_ignorable(value) {
            return !is_cased(value);
        }
    }
    true
}

fn push_mapping(output: &mut String, value: char) {
    if value.is_ascii() {
        output.push(value.to_ascii_lowercase());
        return;
    }
    for &(from, to) in data::MULTI.iter() {
        if from == value {
            output.push_str(to);
            return;
        }
    }
    let point = value as u32;
    let mut low = 0;
    let mut high = data::SIMPLE.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if data::SIMPLE[mid].1 < point {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    if low < data::SIMPLE.len() {
        let (first, _, stride, mapped) = data::SIMPLE[low];
        if point >= first && (point - first).is_multiple_of(stride) {
            // The generator checks every scalar, including stride parity holes.
            output.push(char::from_u32(mapped + point - first).unwrap());
            return;
        }
    }
    output.push(value);
}

pub(super) fn convert(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for (index, value) in input.char_indices() {
        if value == 'Σ' {
            output.push(if final_sigma(input, index) {
                'ς'
            } else {
                'σ'
            });
        } else {
            push_mapping(&mut output, value);
        }
    }
    output
}
