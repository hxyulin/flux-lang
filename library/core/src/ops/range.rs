//! Ranges: what `a..b`, `a..=b`, `a..`, `..b`, `..=b` and `..` make.
//! Those of integers are iterators.

use crate::cmp::PartialOrd;
use crate::fmt::{self, Debug, Formatter};
use crate::result::Result::Ok;

/// `..`: everything.
#[lang = "RangeFull"]
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct RangeFull;

/// `start..end`: from `start`, up to but not including `end`.
#[lang = "Range"]
#[derive(Clone, Default, PartialEq, Eq, Hash)]
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
#[derive(Clone, PartialEq, Eq, Hash)]
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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Clone, PartialEq, Eq, Hash)]
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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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

/// One end of a range: a value in it, a value just past it, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bound<T> {
    /// The range includes this value.
    Included(T),
    /// The range ends just short of this value.
    Excluded(T),
    /// The range has no end here.
    Unbounded,
}

use Bound::{Excluded, Included, Unbounded};

impl<T> Bound<T> {
    /// A bound of a reference to the value.
    pub fn as_ref(&self) -> Bound<&T> {
        match self {
            Included(x) => Included(x),
            Excluded(x) => Excluded(x),
            Unbounded => Unbounded,
        }
    }

    /// `f` of the value.
    pub fn map<U, F: crate::ops::FnOnce(T) -> U>(self, f: F) -> Bound<U> {
        match self {
            Unbounded => Unbounded,
            Included(x) => Included(f(x)),
            Excluded(x) => Excluded(f(x)),
        }
    }
}

impl<T: crate::clone::Clone> Bound<&T> {
    /// A bound of a clone of the value.
    pub fn cloned(self) -> Bound<T> {
        match self {
            Unbounded => Unbounded,
            Included(x) => Included(x.clone()),
            Excluded(x) => Excluded(x.clone()),
        }
    }
}

/// What every range is: a start and an end bound, each a value or none.
pub trait RangeBounds<T> {
    /// Where the range starts.
    fn start_bound(&self) -> Bound<&T>;
    /// Where the range ends.
    fn end_bound(&self) -> Bound<&T>;

    /// Whether `item` is in the range.
    fn contains<U>(&self, item: &U) -> bool
    where
        T: PartialOrd<U>,
        U: PartialOrd<T>,
    {
        (match self.start_bound() {
            Included(start) => start <= item,
            Excluded(start) => start < item,
            Unbounded => true,
        }) && (match self.end_bound() {
            Included(end) => item <= end,
            Excluded(end) => item < end,
            Unbounded => true,
        })
    }
}

impl<T> RangeBounds<T> for RangeFull {
    fn start_bound(&self) -> Bound<&T> {
        Unbounded
    }
    fn end_bound(&self) -> Bound<&T> {
        Unbounded
    }
}

impl<T> RangeBounds<T> for Range<T> {
    fn start_bound(&self) -> Bound<&T> {
        Included(&self.start)
    }
    fn end_bound(&self) -> Bound<&T> {
        Excluded(&self.end)
    }
}

impl<T> RangeBounds<T> for RangeFrom<T> {
    fn start_bound(&self) -> Bound<&T> {
        Included(&self.start)
    }
    fn end_bound(&self) -> Bound<&T> {
        Unbounded
    }
}

impl<T> RangeBounds<T> for RangeTo<T> {
    fn start_bound(&self) -> Bound<&T> {
        Unbounded
    }
    fn end_bound(&self) -> Bound<&T> {
        Excluded(&self.end)
    }
}

impl<T> RangeBounds<T> for RangeInclusive<T> {
    fn start_bound(&self) -> Bound<&T> {
        Included(&self.start)
    }
    /// Once iterated to its end, it ends before it.
    fn end_bound(&self) -> Bound<&T> {
        if self.exhausted { Excluded(&self.end) } else { Included(&self.end) }
    }
}

impl<T> RangeBounds<T> for RangeToInclusive<T> {
    fn start_bound(&self) -> Bound<&T> {
        Unbounded
    }
    fn end_bound(&self) -> Bound<&T> {
        Included(&self.end)
    }
}

impl<T> RangeBounds<T> for (Bound<T>, Bound<T>) {
    fn start_bound(&self) -> Bound<&T> {
        self.0.as_ref()
    }
    fn end_bound(&self) -> Bound<&T> {
        self.1.as_ref()
    }
}
