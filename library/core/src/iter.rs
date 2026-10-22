//! Iterators: values that give one item after another, which `for`
//! loops go through, and the adapters and consumers closures make
//! useful. The ranges of integers are iterators.

use crate::cmp::{Ord, PartialOrd};
use crate::marker::{Copy, Sized};
use crate::ops::{FnMut, Range, RangeFrom, RangeInclusive};
use crate::option::Option::{self, None, Some};

/// Something that gives a sequence of items, one per call of `next`,
/// until it gives `None`.
#[lang = "iterator"]
#[diagnostic::on_unimplemented(message = "`{Self}` is not an iterator")]
pub trait Iterator {
    /// What it gives.
    type Item;

    /// The next item, or `None` once there are no more.
    fn next(&mut self) -> Option<Self::Item>;

    /// Bounds on how many items are left: at least the first, at most
    /// the second if it is `Some`.
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }

    /// How many items are left, going through them.
    fn count(mut self) -> usize
    where
        Self: Sized,
    {
        let mut n = 0;
        while let Some(_) = self.next() {
            n += 1;
        }
        n
    }

    /// The last item.
    fn last(mut self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let mut last = None;
        while let Some(x) = self.next() {
            last = Some(x);
        }
        last
    }

    /// The `n`th item from here, counting from 0.
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let mut n = n;
        while let Some(x) = self.next() {
            if n == 0 {
                return Some(x);
            }
            n -= 1;
        }
        None
    }

    /// Every `step`th item, the first one first.
    fn step_by(self, step: usize) -> StepBy<Self>
    where
        Self: Sized,
    {
        if step == 0 {
            crate::panicking::panic(c"assertion failed: step != 0");
        }
        StepBy {
            iter: self,
            step: step - 1,
            first_take: true,
        }
    }

    /// Its items, then `other`'s.
    fn chain<U: IntoIterator<Item = Self::Item>>(self, other: U) -> Chain<Self, U::IntoIter>
    where
        Self: Sized,
    {
        Chain {
            a: Some(self),
            b: Some(other.into_iter()),
        }
    }

    /// Pairs of its items and `other`'s, until either runs out.
    fn zip<U: IntoIterator>(self, other: U) -> Zip<Self, U::IntoIter>
    where
        Self: Sized,
    {
        Zip {
            a: self,
            b: other.into_iter(),
        }
    }

    /// `f` of each item.
    fn map<B, F: FnMut(Self::Item) -> B>(self, f: F) -> Map<Self, F>
    where
        Self: Sized,
    {
        Map { iter: self, f }
    }

    /// Calls `f` with each item.
    fn for_each<F: FnMut(Self::Item)>(mut self, mut f: F)
    where
        Self: Sized,
    {
        while let Some(x) = self.next() {
            f(x);
        }
    }

    /// The items `predicate` is `true` for.
    fn filter<P: FnMut(&Self::Item) -> bool>(self, predicate: P) -> Filter<Self, P>
    where
        Self: Sized,
    {
        Filter {
            iter: self,
            predicate,
        }
    }

    /// The values in the `Some`s `f` gives for the items.
    fn filter_map<B, F: FnMut(Self::Item) -> Option<B>>(self, f: F) -> FilterMap<Self, F>
    where
        Self: Sized,
    {
        FilterMap { iter: self, f }
    }

    /// Each item with its index, from 0.
    fn enumerate(self) -> Enumerate<Self>
    where
        Self: Sized,
    {
        Enumerate {
            iter: self,
            count: 0,
        }
    }

    /// The items after the first ones `predicate` is `true` for.
    fn skip_while<P: FnMut(&Self::Item) -> bool>(self, predicate: P) -> SkipWhile<Self, P>
    where
        Self: Sized,
    {
        SkipWhile {
            iter: self,
            flag: false,
            predicate,
        }
    }

    /// The first items, while `predicate` is `true` for them.
    fn take_while<P: FnMut(&Self::Item) -> bool>(self, predicate: P) -> TakeWhile<Self, P>
    where
        Self: Sized,
    {
        TakeWhile {
            iter: self,
            flag: false,
            predicate,
        }
    }

    /// The items after the first `n`.
    fn skip(self, n: usize) -> Skip<Self>
    where
        Self: Sized,
    {
        Skip { iter: self, n }
    }

    /// The first `n` items.
    fn take(self, n: usize) -> Take<Self>
    where
        Self: Sized,
    {
        Take { iter: self, n }
    }

    /// `f(f(f(init, a), b), c)` for the items `a`, `b` and `c`.
    fn fold<B, F: FnMut(B, Self::Item) -> B>(mut self, init: B, mut f: F) -> B
    where
        Self: Sized,
    {
        let mut accum = init;
        while let Some(x) = self.next() {
            accum = f(accum, x);
        }
        accum
    }

    /// Whether `f` is `true` for every item, stopping at one it is not.
    fn all<F: FnMut(Self::Item) -> bool>(&mut self, mut f: F) -> bool
    where
        Self: Sized,
    {
        while let Some(x) = self.next() {
            if !f(x) {
                return false;
            }
        }
        true
    }

    /// Whether `f` is `true` for some item, stopping at the first.
    fn any<F: FnMut(Self::Item) -> bool>(&mut self, mut f: F) -> bool
    where
        Self: Sized,
    {
        while let Some(x) = self.next() {
            if f(x) {
                return true;
            }
        }
        false
    }

    /// The first item `predicate` is `true` for.
    fn find<P: FnMut(&Self::Item) -> bool>(&mut self, mut predicate: P) -> Option<Self::Item>
    where
        Self: Sized,
    {
        while let Some(x) = self.next() {
            if predicate(&x) {
                return Some(x);
            }
        }
        None
    }

    /// The first `Some` `f` gives for an item.
    fn find_map<B, F: FnMut(Self::Item) -> Option<B>>(&mut self, mut f: F) -> Option<B>
    where
        Self: Sized,
    {
        while let Some(x) = self.next() {
            if let Some(b) = f(x) {
                return Some(b);
            }
        }
        None
    }

    /// The index of the first item `predicate` is `true` for.
    fn position<P: FnMut(Self::Item) -> bool>(&mut self, mut predicate: P) -> Option<usize>
    where
        Self: Sized,
    {
        let mut i = 0;
        while let Some(x) = self.next() {
            if predicate(x) {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    /// The index of the last item, from the front, for which `predicate`
    /// holds, searching from the back.
    fn rposition<P: FnMut(Self::Item) -> bool>(&mut self, mut predicate: P) -> Option<usize>
    where
        Self: Sized + ExactSizeIterator + DoubleEndedIterator,
    {
        let mut i = self.len();
        while let Some(x) = self.next_back() {
            i -= 1;
            if predicate(x) {
                return Some(i);
            }
        }
        None
    }

    /// The largest item, the last of equal ones.
    fn max(mut self) -> Option<Self::Item>
    where
        Self: Sized,
        Self::Item: Ord,
    {
        let mut best = self.next()?;
        while let Some(x) = self.next() {
            if x >= best {
                best = x;
            }
        }
        Some(best)
    }

    /// The smallest item, the first of equal ones.
    fn min(mut self) -> Option<Self::Item>
    where
        Self: Sized,
        Self::Item: Ord,
    {
        let mut best = self.next()?;
        while let Some(x) = self.next() {
            if x < best {
                best = x;
            }
        }
        Some(best)
    }

    /// The largest item by `compare`, the last of equal ones.
    fn max_by<F: FnMut(&Self::Item, &Self::Item) -> crate::cmp::Ordering>(
        mut self,
        mut compare: F,
    ) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let mut best = self.next()?;
        while let Some(x) = self.next() {
            if !compare(&best, &x).is_gt() {
                best = x;
            }
        }
        Some(best)
    }

    /// The smallest item by `compare`, the first of equal ones.
    fn min_by<F: FnMut(&Self::Item, &Self::Item) -> crate::cmp::Ordering>(
        mut self,
        mut compare: F,
    ) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let mut best = self.next()?;
        while let Some(x) = self.next() {
            if compare(&best, &x).is_gt() {
                best = x;
            }
        }
        Some(best)
    }

    /// The item for which `f` is largest, the last of equal ones.
    fn max_by_key<B: Ord, F: FnMut(&Self::Item) -> B>(mut self, mut f: F) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let mut best = self.next()?;
        let mut best_key = f(&best);
        while let Some(x) = self.next() {
            let key = f(&x);
            if key >= best_key {
                best = x;
                best_key = key;
            }
        }
        Some(best)
    }

    /// The item for which `f` is smallest, the first of equal ones.
    fn min_by_key<B: Ord, F: FnMut(&Self::Item) -> B>(mut self, mut f: F) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let mut best = self.next()?;
        let mut best_key = f(&best);
        while let Some(x) = self.next() {
            let key = f(&x);
            if key < best_key {
                best = x;
                best_key = key;
            }
        }
        Some(best)
    }

    /// The items from the back.
    fn rev(self) -> Rev<Self>
    where
        Self: Sized + DoubleEndedIterator,
    {
        Rev { iter: self }
    }

    /// The items added up.
    fn sum<S: Sum<Self::Item>>(self) -> S
    where
        Self: Sized,
    {
        Sum::sum(self)
    }

    /// The items multiplied.
    fn product<P: Product<Self::Item>>(self) -> P
    where
        Self: Sized,
    {
        Product::product(self)
    }

    /// Whether its items are those of `other`, in order.
    fn eq<I: IntoIterator>(mut self, other: I) -> bool
    where
        Self: Sized,
        Self::Item: crate::cmp::PartialEq<I::Item>,
    {
        let mut other = other.into_iter();
        loop {
            match self.next() {
                None => return other.next().is_none(),
                Some(x) => match other.next() {
                    None => return false,
                    Some(y) => {
                        if x != y {
                            return false;
                        }
                    }
                },
            }
        }
    }
}

/// An iterator that knows how many items it has left.
pub trait ExactSizeIterator: Iterator {
    /// How many items are left.
    fn len(&self) -> usize {
        let (lower, _) = self.size_hint();
        lower
    }
}

/// An iterator that can give its items from the back too.
pub trait DoubleEndedIterator: Iterator {
    /// The last item, taken off the back.
    fn next_back(&mut self) -> Option<Self::Item>;

    /// The `n`th item from the back.
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let mut n = n;
        while let Some(x) = self.next_back() {
            if n == 0 {
                return Some(x);
            }
            n -= 1;
        }
        None
    }

    /// The first item from the back `predicate` is `true` for.
    fn rfind<P: FnMut(&Self::Item) -> bool>(&mut self, mut predicate: P) -> Option<Self::Item>
    where
        Self: Sized,
    {
        while let Some(x) = self.next_back() {
            if predicate(&x) {
                return Some(x);
            }
        }
        None
    }
}

/// What `for` loops go through: an iterator, or what makes one.
#[lang = "IntoIterator"]
#[diagnostic::on_unimplemented(message = "`{Self}` is not an iterator")]
pub trait IntoIterator {
    /// What the iterator gives.
    type Item;
    /// The iterator.
    type IntoIter: Iterator<Item = Self::Item>;

    fn into_iter(self) -> Self::IntoIter;
}

/// An iterator is its own.
impl<I: Iterator> IntoIterator for I {
    type Item = I::Item;
    type IntoIter = I;

    fn into_iter(self) -> I {
        self
    }
}

/// What `Iterator::sum` adds up into.
pub trait Sum<A = Self>: Sized {
    fn sum<I: Iterator<Item = A>>(iter: I) -> Self;
}

/// What `Iterator::product` multiplies into.
pub trait Product<A = Self>: Sized {
    fn product<I: Iterator<Item = A>>(iter: I) -> Self;
}

/// What ranges step through: the integer types.
pub trait Step: Copy + PartialOrd<Self> {
    /// The next value, wrapping.
    fn forward_one(self) -> Self;
    /// The value before.
    fn backward_one(self) -> Self;
}

impl<A: Step> Iterator for Range<A> {
    type Item = A;

    fn next(&mut self) -> Option<A> {
        if self.start < self.end {
            let n = self.start;
            self.start = n.forward_one();
            Some(n)
        } else {
            None
        }
    }
}

impl<A: Step> DoubleEndedIterator for Range<A> {
    fn next_back(&mut self) -> Option<A> {
        if self.start < self.end {
            self.end = self.end.backward_one();
            Some(self.end)
        } else {
            None
        }
    }
}

impl<A: Step> Iterator for RangeFrom<A> {
    type Item = A;

    fn next(&mut self) -> Option<A> {
        let n = self.start;
        self.start = n.forward_one();
        Some(n)
    }
}

impl<A: Step> Iterator for RangeInclusive<A> {
    type Item = A;

    fn next(&mut self) -> Option<A> {
        if self.exhausted || !(self.start <= self.end) {
            return None;
        }
        let n = self.start;
        if n < self.end {
            self.start = n.forward_one();
        } else {
            self.exhausted = true;
        }
        Some(n)
    }
}

impl<A: Step> DoubleEndedIterator for RangeInclusive<A> {
    fn next_back(&mut self) -> Option<A> {
        if self.exhausted || !(self.start <= self.end) {
            return None;
        }
        let n = self.end;
        if self.start < n {
            self.end = n.backward_one();
        } else {
            self.exhausted = true;
        }
        Some(n)
    }
}

/// [`Iterator::map`]'s.
#[derive(Clone)]
pub struct Map<I, F> {
    iter: I,
    f: F,
}

impl<B, I: Iterator, F: FnMut(I::Item) -> B> Iterator for Map<I, F> {
    type Item = B;

    fn next(&mut self) -> Option<B> {
        match self.iter.next() {
            Some(x) => Some((self.f)(x)),
            None => None,
        }
    }
}

impl<B, I: DoubleEndedIterator, F: FnMut(I::Item) -> B> DoubleEndedIterator for Map<I, F> {
    fn next_back(&mut self) -> Option<B> {
        match self.iter.next_back() {
            Some(x) => Some((self.f)(x)),
            None => None,
        }
    }
}

/// [`Iterator::filter`]'s.
#[derive(Clone)]
pub struct Filter<I, P> {
    iter: I,
    predicate: P,
}

impl<I: Iterator, P: FnMut(&I::Item) -> bool> Iterator for Filter<I, P> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        while let Some(x) = self.iter.next() {
            if (self.predicate)(&x) {
                return Some(x);
            }
        }
        None
    }
}

impl<I: DoubleEndedIterator, P: FnMut(&I::Item) -> bool> DoubleEndedIterator for Filter<I, P> {
    fn next_back(&mut self) -> Option<I::Item> {
        while let Some(x) = self.iter.next_back() {
            if (self.predicate)(&x) {
                return Some(x);
            }
        }
        None
    }
}

/// [`Iterator::filter_map`]'s.
#[derive(Clone)]
pub struct FilterMap<I, F> {
    iter: I,
    f: F,
}

impl<B, I: Iterator, F: FnMut(I::Item) -> Option<B>> Iterator for FilterMap<I, F> {
    type Item = B;

    fn next(&mut self) -> Option<B> {
        while let Some(x) = self.iter.next() {
            if let Some(y) = (self.f)(x) {
                return Some(y);
            }
        }
        None
    }
}

/// [`Iterator::enumerate`]'s.
#[derive(Clone)]
pub struct Enumerate<I> {
    iter: I,
    count: usize,
}

impl<I: Iterator> Iterator for Enumerate<I> {
    type Item = (usize, I::Item);

    fn next(&mut self) -> Option<(usize, I::Item)> {
        let a = self.iter.next()?;
        let i = self.count;
        self.count += 1;
        Some((i, a))
    }
}

/// [`Iterator::zip`]'s.
#[derive(Clone)]
pub struct Zip<A, B> {
    a: A,
    b: B,
}

impl<A: Iterator, B: Iterator> Iterator for Zip<A, B> {
    type Item = (A::Item, B::Item);

    fn next(&mut self) -> Option<(A::Item, B::Item)> {
        let x = self.a.next()?;
        let y = self.b.next()?;
        Some((x, y))
    }
}

/// [`Iterator::chain`]'s.
#[derive(Clone)]
pub struct Chain<A, B> {
    a: Option<A>,
    b: Option<B>,
}

impl<A: Iterator, B: Iterator<Item = A::Item>> Iterator for Chain<A, B> {
    type Item = A::Item;

    fn next(&mut self) -> Option<A::Item> {
        if let Some(a) = &mut self.a {
            match a.next() {
                Some(x) => return Some(x),
                None => self.a = None,
            }
        }
        match &mut self.b {
            Some(b) => b.next(),
            None => None,
        }
    }
}

/// [`Iterator::take`]'s.
#[derive(Clone)]
pub struct Take<I> {
    iter: I,
    n: usize,
}

impl<I: Iterator> Iterator for Take<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        if self.n != 0 {
            self.n -= 1;
            self.iter.next()
        } else {
            None
        }
    }
}

/// [`Iterator::skip`]'s.
#[derive(Clone)]
pub struct Skip<I> {
    iter: I,
    n: usize,
}

impl<I: Iterator> Iterator for Skip<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        while self.n > 0 {
            self.n -= 1;
            self.iter.next()?;
        }
        self.iter.next()
    }
}

/// [`Iterator::take_while`]'s.
#[derive(Clone)]
pub struct TakeWhile<I, P> {
    iter: I,
    flag: bool,
    predicate: P,
}

impl<I: Iterator, P: FnMut(&I::Item) -> bool> Iterator for TakeWhile<I, P> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        if self.flag {
            return None;
        }
        let x = self.iter.next()?;
        if (self.predicate)(&x) {
            Some(x)
        } else {
            self.flag = true;
            None
        }
    }
}

/// [`Iterator::skip_while`]'s.
#[derive(Clone)]
pub struct SkipWhile<I, P> {
    iter: I,
    flag: bool,
    predicate: P,
}

impl<I: Iterator, P: FnMut(&I::Item) -> bool> Iterator for SkipWhile<I, P> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        loop {
            let x = self.iter.next()?;
            if self.flag || !(self.predicate)(&x) {
                self.flag = true;
                return Some(x);
            }
        }
    }
}

/// [`Iterator::step_by`]'s.
#[derive(Clone)]
pub struct StepBy<I> {
    iter: I,
    step: usize,
    first_take: bool,
}

impl<I: Iterator> Iterator for StepBy<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        if self.first_take {
            self.first_take = false;
            self.iter.next()
        } else {
            self.iter.nth(self.step)
        }
    }
}

/// [`Iterator::rev`]'s.
#[derive(Clone)]
pub struct Rev<I> {
    iter: I,
}

impl<I: DoubleEndedIterator> Iterator for Rev<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        self.iter.next_back()
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for Rev<I> {
    fn next_back(&mut self) -> Option<I::Item> {
        self.iter.next()
    }
}

// The integer types step, add up and multiply, written out for each
// since flux has no macros.
impl Step for i8 { fn forward_one(self) -> i8 { self.wrapping_add(1) } fn backward_one(self) -> i8 { self.wrapping_sub(1) } }
impl Sum for i8 { fn sum<I: Iterator<Item = i8>>(mut iter: I) -> i8 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for i8 { fn product<I: Iterator<Item = i8>>(mut iter: I) -> i8 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for i16 { fn forward_one(self) -> i16 { self.wrapping_add(1) } fn backward_one(self) -> i16 { self.wrapping_sub(1) } }
impl Sum for i16 { fn sum<I: Iterator<Item = i16>>(mut iter: I) -> i16 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for i16 { fn product<I: Iterator<Item = i16>>(mut iter: I) -> i16 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for i32 { fn forward_one(self) -> i32 { self.wrapping_add(1) } fn backward_one(self) -> i32 { self.wrapping_sub(1) } }
impl Sum for i32 { fn sum<I: Iterator<Item = i32>>(mut iter: I) -> i32 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for i32 { fn product<I: Iterator<Item = i32>>(mut iter: I) -> i32 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for i64 { fn forward_one(self) -> i64 { self.wrapping_add(1) } fn backward_one(self) -> i64 { self.wrapping_sub(1) } }
impl Sum for i64 { fn sum<I: Iterator<Item = i64>>(mut iter: I) -> i64 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for i64 { fn product<I: Iterator<Item = i64>>(mut iter: I) -> i64 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for i128 { fn forward_one(self) -> i128 { self.wrapping_add(1) } fn backward_one(self) -> i128 { self.wrapping_sub(1) } }
impl Sum for i128 { fn sum<I: Iterator<Item = i128>>(mut iter: I) -> i128 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for i128 { fn product<I: Iterator<Item = i128>>(mut iter: I) -> i128 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for isize { fn forward_one(self) -> isize { self.wrapping_add(1) } fn backward_one(self) -> isize { self.wrapping_sub(1) } }
impl Sum for isize { fn sum<I: Iterator<Item = isize>>(mut iter: I) -> isize { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for isize { fn product<I: Iterator<Item = isize>>(mut iter: I) -> isize { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for u8 { fn forward_one(self) -> u8 { self.wrapping_add(1) } fn backward_one(self) -> u8 { self.wrapping_sub(1) } }
impl Sum for u8 { fn sum<I: Iterator<Item = u8>>(mut iter: I) -> u8 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for u8 { fn product<I: Iterator<Item = u8>>(mut iter: I) -> u8 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for u16 { fn forward_one(self) -> u16 { self.wrapping_add(1) } fn backward_one(self) -> u16 { self.wrapping_sub(1) } }
impl Sum for u16 { fn sum<I: Iterator<Item = u16>>(mut iter: I) -> u16 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for u16 { fn product<I: Iterator<Item = u16>>(mut iter: I) -> u16 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for u32 { fn forward_one(self) -> u32 { self.wrapping_add(1) } fn backward_one(self) -> u32 { self.wrapping_sub(1) } }
impl Sum for u32 { fn sum<I: Iterator<Item = u32>>(mut iter: I) -> u32 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for u32 { fn product<I: Iterator<Item = u32>>(mut iter: I) -> u32 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for u64 { fn forward_one(self) -> u64 { self.wrapping_add(1) } fn backward_one(self) -> u64 { self.wrapping_sub(1) } }
impl Sum for u64 { fn sum<I: Iterator<Item = u64>>(mut iter: I) -> u64 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for u64 { fn product<I: Iterator<Item = u64>>(mut iter: I) -> u64 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for u128 { fn forward_one(self) -> u128 { self.wrapping_add(1) } fn backward_one(self) -> u128 { self.wrapping_sub(1) } }
impl Sum for u128 { fn sum<I: Iterator<Item = u128>>(mut iter: I) -> u128 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for u128 { fn product<I: Iterator<Item = u128>>(mut iter: I) -> u128 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl Step for usize { fn forward_one(self) -> usize { self.wrapping_add(1) } fn backward_one(self) -> usize { self.wrapping_sub(1) } }
impl Sum for usize { fn sum<I: Iterator<Item = usize>>(mut iter: I) -> usize { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl Product for usize { fn product<I: Iterator<Item = usize>>(mut iter: I) -> usize { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
