//! Ranges: what `a..b`, `a..=b`, `a..`, `..b`, `..=b` and `..` make.
//! Those of integers are iterators.

use crate::cmp::PartialOrd;

/// `..`: everything.
#[lang = "RangeFull"]
#[derive(Clone, Debug, Copy, Default, PartialEq, Eq)]
pub struct RangeFull;

/// `start..end`: from `start`, up to but not including `end`.
#[lang = "Range"]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Range<Idx> {
    pub start: Idx,
    pub end: Idx,
}

impl<Idx: PartialOrd<Idx>> Range<Idx> {
    /// Whether `item` is in the range.
    pub fn contains(&self, item: &Idx) -> bool {
        self.start <= *item && *item < self.end
    }

    /// Whether the range has nothing in it.
    pub fn is_empty(&self) -> bool {
        !(self.start < self.end)
    }
}

/// `start..`: from `start` on.
#[lang = "RangeFrom"]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeFrom<Idx> {
    pub start: Idx,
}

impl<Idx: PartialOrd<Idx>> RangeFrom<Idx> {
    /// Whether `item` is in the range.
    pub fn contains(&self, item: &Idx) -> bool {
        self.start <= *item
    }
}

/// `..end`: up to but not including `end`.
#[lang = "RangeTo"]
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub struct RangeTo<Idx> {
    pub end: Idx,
}

impl<Idx: PartialOrd<Idx>> RangeTo<Idx> {
    /// Whether `item` is in the range.
    pub fn contains(&self, item: &Idx) -> bool {
        *item < self.end
    }
}

/// `start..=end`: from `start` up to and including `end`. Once iterated
/// to its end, it is `exhausted`.
#[lang = "RangeInclusive"]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeInclusive<Idx> {
    pub(crate) start: Idx,
    pub(crate) end: Idx,
    pub(crate) exhausted: bool,
}

impl<Idx> RangeInclusive<Idx> {
    /// What `start..=end` makes.
    pub const fn new(start: Idx, end: Idx) -> Self {
        RangeInclusive {
            start,
            end,
            exhausted: false,
        }
    }
}

impl<Idx: PartialOrd<Idx>> RangeInclusive<Idx> {
    /// Whether `item` is in the range.
    pub fn contains(&self, item: &Idx) -> bool {
        self.start <= *item && *item <= self.end
    }

    /// Whether the range has nothing (left) in it.
    pub fn is_empty(&self) -> bool {
        self.exhausted || !(self.start <= self.end)
    }
}

/// `..=end`: up to and including `end`.
#[lang = "RangeToInclusive"]
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub struct RangeToInclusive<Idx> {
    pub end: Idx,
}

impl<Idx: PartialOrd<Idx>> RangeToInclusive<Idx> {
    /// Whether `item` is in the range.
    pub fn contains(&self, item: &Idx) -> bool {
        *item <= self.end
    }
}
