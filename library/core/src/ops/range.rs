//! Ranges: what `a..b`, `a..=b`, `a..`, `..b`, `..=b` and `..` make.
//! Those of integers are iterators.

use crate::cmp::PartialOrd;
use crate::fmt::{self, Debug, Formatter};
use crate::result::Result::Ok;

/// `..`: everything.
#[lang = "RangeFull"]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct RangeFull;

/// `start..end`: from `start`, up to but not including `end`.
#[lang = "Range"]
#[derive(Clone, Default, PartialEq, Eq)]
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Clone, Copy, PartialEq, Eq)]
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
#[derive(Clone, PartialEq, Eq)]
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
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RangeToInclusive<Idx> {
    pub end: Idx,
}

impl<Idx: PartialOrd<Idx>> RangeToInclusive<Idx> {
    /// Whether `item` is in the range.
    pub fn contains(&self, item: &Idx) -> bool {
        *item <= self.end
    }
}

// As rustc writes them: `1..5`, `..=3`, `..`.
impl Debug for RangeFull {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("..")
    }
}

impl<Idx: Debug> Debug for Range<Idx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.start.fmt(f)?;
        f.write_str("..")?;
        self.end.fmt(f)
    }
}

impl<Idx: Debug> Debug for RangeFrom<Idx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.start.fmt(f)?;
        f.write_str("..")
    }
}

impl<Idx: Debug> Debug for RangeTo<Idx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("..")?;
        self.end.fmt(f)
    }
}

impl<Idx: Debug> Debug for RangeInclusive<Idx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.start.fmt(f)?;
        f.write_str("..=")?;
        self.end.fmt(f)?;
        if self.exhausted {
            f.write_str(" (exhausted)")?;
        }
        Ok(())
    }
}

impl<Idx: Debug> Debug for RangeToInclusive<Idx> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("..=")?;
        self.end.fmt(f)
    }
}
