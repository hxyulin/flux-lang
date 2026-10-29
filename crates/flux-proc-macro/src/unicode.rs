//! Unicode identifiers and NFC normalization without host dependencies.

use super::unicode_data::{CLASSES, COMPOSITIONS, DECOMPOSITIONS, XID_CONTINUE, XID_START};

fn in_ranges(ch: char, ranges: &[(u32, u32)]) -> bool {
    let ch = ch as u32;
    let index = ranges.partition_point(|&(start, _)| start <= ch);
    index != 0 && ch <= ranges[index - 1].1
}

pub(super) fn is_start(ch: char) -> bool {
    ch == '_' || in_ranges(ch, XID_START)
}
pub(super) fn is_continue(ch: char) -> bool {
    in_ranges(ch, XID_CONTINUE)
}

fn class(ch: char) -> u8 {
    CLASSES
        .binary_search_by_key(&(ch as u32), |&(code, _)| code)
        .map_or(0, |index| CLASSES[index].1)
}

fn compose(a: char, b: char) -> Option<char> {
    let (a, b) = (a as u32, b as u32);
    // Unicode's algorithmic Hangul composition, UAX #15 section 3.12.
    if (0x1100..0x1113).contains(&a) && (0x1161..0x1176).contains(&b) {
        return char::from_u32(0xAC00 + (a - 0x1100) * 588 + (b - 0x1161) * 28);
    }
    if (0xAC00..0xD7A4).contains(&a)
        && (a - 0xAC00).is_multiple_of(28)
        && (0x11A8..0x11C3).contains(&b)
    {
        return char::from_u32(a + b - 0x11A7);
    }
    COMPOSITIONS
        .binary_search_by_key(&(a, b), |&(pair, _)| pair)
        .ok()
        .map(|index| COMPOSITIONS[index].1)
}

pub(super) fn normalize(text: &str) -> String {
    if text.is_ascii() {
        return text.to_owned();
    }
    let mut decomposed = Vec::new();
    for ch in text.chars() {
        let code = ch as u32;
        if (0xAC00..0xD7A4).contains(&code) {
            let syllable = code - 0xAC00;
            decomposed.push(char::from_u32(0x1100 + syllable / 588).expect("Hangul leading jamo"));
            decomposed
                .push(char::from_u32(0x1161 + (syllable % 588) / 28).expect("Hangul vowel jamo"));
            let trailing = syllable % 28;
            if trailing != 0 {
                decomposed.push(char::from_u32(0x11A7 + trailing).expect("Hangul trailing jamo"));
            }
        } else if let Ok(index) = DECOMPOSITIONS.binary_search_by_key(&code, |&(code, _)| code) {
            decomposed.extend_from_slice(DECOMPOSITIONS[index].1);
        } else {
            decomposed.push(ch);
        }
    }
    // Stable sorting preserves equal-class mark order. Each starter terminates
    // the preceding combining sequence.
    let mut sequence = 0;
    for index in 0..decomposed.len() {
        if class(decomposed[index]) == 0 {
            decomposed[sequence..index].sort_by_key(|&ch| class(ch));
            sequence = index + 1;
        }
    }
    decomposed[sequence..].sort_by_key(|&ch| class(ch));
    let mut output = Vec::new();
    let mut starter = None;
    let mut last_class = 0;
    for ch in decomposed {
        let next_class = class(ch);
        if let Some(index) = starter
            && (last_class == 0 || last_class < next_class)
            && let Some(composed) = compose(output[index], ch)
        {
            output[index] = composed;
            continue;
        }
        if next_class == 0 {
            starter = Some(output.len());
        }
        last_class = next_class;
        output.push(ch);
    }
    output.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_normalization::UnicodeNormalization;

    #[test]
    fn xid_tables_match_reference_for_every_scalar() {
        for ch in (0..=0x10ffff).filter_map(char::from_u32) {
            assert_eq!(
                is_start(ch),
                ch == '_' || unicode_ident::is_xid_start(ch),
                "{ch:?}"
            );
            assert_eq!(
                is_continue(ch),
                unicode_ident::is_xid_continue(ch),
                "{ch:?}"
            );
        }
    }

    #[test]
    fn canonical_normalization_matches_reference() {
        for ch in (0..=0x10ffff).filter_map(char::from_u32) {
            if class(ch) != 0
                || DECOMPOSITIONS
                    .binary_search_by_key(&(ch as u32), |&(code, _)| code)
                    .is_ok()
                || ('\u{AC00}'..='\u{D7A3}').contains(&ch)
            {
                let input = format!("a{ch}\u{301}\u{327}");
                assert_eq!(normalize(&input), input.nfc().collect::<String>(), "{ch:?}");
                let input = ch.to_string();
                assert_eq!(normalize(&input), input.nfc().collect::<String>(), "{ch:?}");
            }
        }
        for input in [
            "e\u{301}",
            "A\u{30A}\u{301}",
            "\u{1100}\u{1161}\u{11A8}",
            "\u{301}\u{327}a",
            "a\u{301}\u{300}",
        ] {
            assert_eq!(
                normalize(input),
                input.nfc().collect::<String>(),
                "{input:?}"
            );
        }
    }
}
