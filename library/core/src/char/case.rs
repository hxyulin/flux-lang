//! Context-free Unicode char casing, distinct from whole-string final sigma.
#[path = "case_data.rs"]
mod data;

#[derive(Clone)]
struct Mapping {
    chars: [char; 3],
    front: usize,
    end: usize,
}
struct Remaining<'a>(&'a [char]);
impl crate::fmt::Debug for Remaining<'_> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.debug_tuple("IntoIter").field(&self.0).finish()
    }
}
impl crate::fmt::Debug for Mapping {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.debug_tuple("CaseMappingIter")
            .field(&Remaining(&self.chars[self.front..self.end]))
            .finish()
    }
}
impl Mapping {
    fn new(chars: [char; 3]) -> Self {
        let end = if chars[2] != '\0' {
            3
        } else if chars[1] != '\0' {
            2
        } else {
            1
        };
        Self {
            chars,
            front: 0,
            end,
        }
    }
    fn next(&mut self) -> Option<char> {
        if self.front == self.end {
            return None;
        }
        let ch = self.chars[self.front];
        self.front += 1;
        Some(ch)
    }
    fn next_back(&mut self) -> Option<char> {
        if self.front == self.end {
            return None;
        }
        self.end -= 1;
        Some(self.chars[self.end])
    }
}

macro_rules! case_iterator {
    ($name:ident) => {
        #[derive(Clone, Debug)]
        pub struct $name(Mapping);
        impl Iterator for $name {
            type Item = char;
            fn next(&mut self) -> Option<char> {
                self.0.next()
            }
            fn size_hint(&self) -> (usize, Option<usize>) {
                let len = self.0.end - self.0.front;
                (len, Some(len))
            }
        }
        impl DoubleEndedIterator for $name {
            fn next_back(&mut self) -> Option<char> {
                self.0.next_back()
            }
        }
        impl ExactSizeIterator for $name {
            fn len(&self) -> usize {
                self.0.end - self.0.front
            }
        }
        impl crate::iter::FusedIterator for $name {}
        impl crate::fmt::Display for $name {
            fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
                for index in self.0.front..self.0.end {
                    crate::fmt::Display::fmt(&self.0.chars[index], f)?;
                }
                Ok(())
            }
        }
    };
}
case_iterator!(ToLowercase);
case_iterator!(ToUppercase);

fn map(value: char, ranges: &[(u32, u32, u32, u32)], multi: &[(u32, [u32; 3])]) -> [char; 3] {
    let point = value as u32;
    for &(from, mapped) in multi {
        if point == from {
            return [
                char::from_u32(mapped[0]).unwrap(),
                char::from_u32(mapped[1]).unwrap(),
                char::from_u32(mapped[2]).unwrap(),
            ];
        }
    }
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
    if low < ranges.len() {
        let (first, _, stride, mapped) = ranges[low];
        if point >= first && (point - first).is_multiple_of(stride) {
            return [char::from_u32(mapped + point - first).unwrap(), '\0', '\0'];
        }
    }
    [value, '\0', '\0']
}

pub(super) fn lower(value: char) -> ToLowercase {
    ToLowercase(Mapping::new(if value.is_ascii() {
        [value.to_ascii_lowercase(), '\0', '\0']
    } else {
        map(value, &data::LOWER, &data::LOWER_MULTI)
    }))
}
pub(super) fn upper(value: char) -> ToUppercase {
    ToUppercase(Mapping::new(if value.is_ascii() {
        [value.to_ascii_uppercase(), '\0', '\0']
    } else {
        map(value, &data::UPPER, &data::UPPER_MULTI)
    }))
}
