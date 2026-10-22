//! Iterators: values that give one item after another, which `for`
//! loops go through, and the adapters and consumers closures make
//! useful. The ranges of integers are iterators.

use crate::clone::Clone;
use crate::cmp::{Ord, PartialEq, PartialOrd};
use crate::marker::{Copy, PhantomData, Sized};
use crate::ops::{ControlFlow, FnMut, FnOnce, FromResidual, Range, RangeFrom, RangeInclusive, Try};
use crate::option::Option::{self, None, Some};
use crate::result::Result::{self, Err, Ok};

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
            crate::panicking::panic("assertion failed: step != 0");
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

    /// A collection of the items: whatever implements `FromIterator`
    /// for them.
    fn collect<B: FromIterator<Self::Item>>(self) -> B
    where
        Self: Sized,
    {
        B::from_iter(self)
    }

    /// The items, pairs, split into two collections.
    fn unzip<A, B, FromA, FromB>(self) -> (FromA, FromB)
    where
        FromA: crate::default::Default + Extend<A>,
        FromB: crate::default::Default + Extend<B>,
        Self: Sized + Iterator<Item = (A, B)>,
    {
        let mut unzipped: (FromA, FromB) = (FromA::default(), FromB::default());
        unzipped.extend(self);
        unzipped
    }

    /// The items `f` is `true` for, and the others, each in a collection.
    fn partition<B, F>(mut self, mut f: F) -> (B, B)
    where
        Self: Sized,
        B: crate::default::Default + Extend<Self::Item>,
        F: FnMut(&Self::Item) -> bool,
    {
        let mut left: B = B::default();
        let mut right: B = B::default();
        while let Some(x) = self.next() {
            if f(&x) {
                left.extend(Some(x));
            } else {
                right.extend(Some(x));
            }
        }
        (left, right)
    }

    /// `fold` with an `f` that may stop early: an `Option`, `Result` or
    /// `ControlFlow` whose `None`, `Err` or `Break` is returned at once.
    fn try_fold<B, F, R>(&mut self, init: B, mut f: F) -> R
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> R,
        R: Try<Output = B>,
    {
        let mut accum = init;
        while let Some(x) = self.next() {
            match f(accum, x).branch() {
                ControlFlow::Continue(c) => accum = c,
                ControlFlow::Break(r) => return R::from_residual(r),
            }
        }
        R::from_output(accum)
    }

    /// Calls `f` with each item, stopping at the first `None`, `Err` or
    /// `Break` it gives, which it returns.
    fn try_for_each<F, R>(&mut self, mut f: F) -> R
    where
        Self: Sized,
        F: FnMut(Self::Item) -> R,
        R: Try<Output = ()>,
    {
        while let Some(x) = self.next() {
            match f(x).branch() {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(r) => return R::from_residual(r),
            }
        }
        R::from_output(())
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

    /// The items folded with `f`, the first as the start: `None` if
    /// there are none.
    fn reduce<F: FnMut(Self::Item, Self::Item) -> Self::Item>(mut self, f: F) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let first = self.next()?;
        Some(self.fold(first, f))
    }

    /// Borrows the iterator: adapters of the borrow leave it usable.
    fn by_ref(&mut self) -> &mut Self
    where
        Self: Sized,
    {
        self
    }

    /// Copies of the values its items refer to.
    fn copied<'a, T: Copy>(self) -> Copied<Self>
    where
        Self: Sized + Iterator<Item = &'a T>,
    {
        Copied { it: self }
    }

    /// Clones of the values its items refer to.
    fn cloned<'a, T: Clone>(self) -> Cloned<Self>
    where
        Self: Sized + Iterator<Item = &'a T>,
    {
        Cloned { it: self }
    }

    /// The items of the iterators `f` makes of its items, in turn.
    fn flat_map<U: IntoIterator, F: FnMut(Self::Item) -> U>(self, f: F) -> FlatMap<Self, U, F>
    where
        Self: Sized,
    {
        FlatMap {
            iter: self.map(f).fuse(),
            frontiter: None,
            backiter: None,
        }
    }

    /// The items of its items, which are iterators or make them, in turn.
    fn flatten(self) -> Flatten<Self>
    where
        Self: Sized,
        Self::Item: IntoIterator,
    {
        Flatten {
            iter: self.fuse(),
            frontiter: None,
            backiter: None,
        }
    }

    /// An iterator that can look at its next item without taking it.
    fn peekable(self) -> Peekable<Self>
    where
        Self: Sized,
    {
        Peekable {
            iter: self,
            peeked: None,
        }
    }

    /// The values in the `Some`s `f` gives, with a state it may change,
    /// until it gives `None`.
    fn scan<St, B, F: FnMut(&mut St, Self::Item) -> Option<B>>(
        self,
        initial_state: St,
        f: F,
    ) -> Scan<Self, St, F>
    where
        Self: Sized,
    {
        Scan {
            iter: self,
            f,
            state: initial_state,
        }
    }

    /// The values in the `Some`s `predicate` gives, until it gives `None`.
    fn map_while<B, P: FnMut(Self::Item) -> Option<B>>(self, predicate: P) -> MapWhile<Self, P>
    where
        Self: Sized,
    {
        MapWhile {
            iter: self,
            predicate,
        }
    }

    /// Its items, calling `f` with a reference to each first.
    fn inspect<F: FnMut(&Self::Item)>(self, f: F) -> Inspect<Self, F>
    where
        Self: Sized,
    {
        Inspect { iter: self, f }
    }

    /// Its items, over and over: a clone of it, from the start, each time
    /// it runs out.
    fn cycle(self) -> Cycle<Self>
    where
        Self: Sized + Clone,
    {
        Cycle {
            orig: self.clone(),
            iter: self,
        }
    }

    /// Its items, then `None` forever once it gives one.
    fn fuse(self) -> Fuse<Self>
    where
        Self: Sized,
    {
        Fuse { iter: Some(self) }
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

    /// `try_fold` from the back.
    fn try_rfold<B, F, R>(&mut self, init: B, mut f: F) -> R
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> R,
        R: Try<Output = B>,
    {
        let mut accum = init;
        while let Some(x) = self.next_back() {
            match f(accum, x).branch() {
                ControlFlow::Continue(c) => accum = c,
                ControlFlow::Break(r) => return R::from_residual(r),
            }
        }
        R::from_output(accum)
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

/// An iterator that knows how many items it has left.
pub trait ExactSizeIterator: Iterator {
    /// How many items are left.
    fn len(&self) -> usize {
        let (lower, upper) = self.size_hint();
        if upper != Some(lower) {
            crate::panicking::panic("assertion `left == right` failed");
        }
        lower
    }
}

/// A mutable reference to an iterator is one.
impl<I: Iterator + ?Sized> Iterator for &mut I {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        (**self).next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (**self).size_hint()
    }

    fn nth(&mut self, n: usize) -> Option<I::Item> {
        (**self).nth(n)
    }
}

impl<I: DoubleEndedIterator + ?Sized> DoubleEndedIterator for &mut I {
    fn next_back(&mut self) -> Option<I::Item> {
        (**self).next_back()
    }
}

impl<I: ExactSizeIterator + ?Sized> ExactSizeIterator for &mut I {
    fn len(&self) -> usize {
        (**self).len()
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
#[diagnostic::on_unimplemented(
    message = "a value of type `{Self}` cannot be made by summing an iterator over elements of type `{A}`"
)]
pub trait Sum<A = Self>: Sized {
    fn sum<I: Iterator<Item = A>>(iter: I) -> Self;
}

/// What `Iterator::product` multiplies into.
#[diagnostic::on_unimplemented(
    message = "a value of type `{Self}` cannot be made by multiplying all elements of type `{A}` from an iterator"
)]
pub trait Product<A = Self>: Sized {
    fn product<I: Iterator<Item = A>>(iter: I) -> Self;
}

/// What ranges step through: the integer types.
pub trait Step: Copy + PartialOrd<Self> {
    /// How many steps from `start` to `end`, as `size_hint` gives
    /// them: `(0, None)` if `end` is before `start`, `(usize::MAX,
    /// None)` if there are more than `usize` holds.
    fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>);
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.start < self.end {
            Step::steps_between(&self.start, &self.end)
        } else {
            (0, Some(0))
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::MAX, None)
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.exhausted || !(self.start <= self.end) {
            return (0, Some(0));
        }
        let (lower, upper) = Step::steps_between(&self.start, &self.end);
        let upper = match upper {
            Some(steps) => steps.checked_add(1),
            None => None,
        };
        (lower.saturating_add(1), upper)
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper)
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper)
    }
}

impl<B, I: DoubleEndedIterator, F: FnMut(I::Item) -> Option<B>> DoubleEndedIterator
    for FilterMap<I, F>
{
    fn next_back(&mut self) -> Option<B> {
        while let Some(x) = self.iter.next_back() {
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<I: ExactSizeIterator + DoubleEndedIterator> DoubleEndedIterator for Enumerate<I> {
    fn next_back(&mut self) -> Option<(usize, I::Item)> {
        let a = self.iter.next_back()?;
        let len = self.iter.len();
        Some((self.count + len, a))
    }
}

impl<I: ExactSizeIterator> ExactSizeIterator for Enumerate<I> {
    fn len(&self) -> usize {
        self.iter.len()
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (a_lower, a_upper) = self.a.size_hint();
        let (b_lower, b_upper) = self.b.size_hint();
        let lower = crate::cmp::min(a_lower, b_lower);
        let upper = match (a_upper, b_upper) {
            (Some(x), Some(y)) => Some(crate::cmp::min(x, y)),
            (Some(x), None) => Some(x),
            (None, Some(y)) => Some(y),
            (None, None) => None,
        };
        (lower, upper)
    }
}

impl<A: ExactSizeIterator, B: ExactSizeIterator> ExactSizeIterator for Zip<A, B> {}

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

    fn size_hint(&self) -> (usize, Option<usize>) {
        match (&self.a, &self.b) {
            (Some(a), Some(b)) => {
                let (a_lower, a_upper) = a.size_hint();
                let (b_lower, b_upper) = b.size_hint();
                let lower = a_lower.saturating_add(b_lower);
                let upper = match (a_upper, b_upper) {
                    (Some(x), Some(y)) => x.checked_add(y),
                    _ => None,
                };
                (lower, upper)
            }
            (Some(a), None) => a.size_hint(),
            (None, Some(b)) => b.size_hint(),
            (None, None) => (0, Some(0)),
        }
    }
}

impl<A: DoubleEndedIterator, B: DoubleEndedIterator<Item = A::Item>> DoubleEndedIterator
    for Chain<A, B>
{
    fn next_back(&mut self) -> Option<A::Item> {
        if let Some(b) = &mut self.b {
            match b.next_back() {
                Some(x) => return Some(x),
                None => self.b = None,
            }
        }
        match &mut self.a {
            Some(a) => a.next_back(),
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.n == 0 {
            return (0, Some(0));
        }
        let (lower, upper) = self.iter.size_hint();
        let lower = crate::cmp::min(lower, self.n);
        let upper = match upper {
            Some(x) if x < self.n => Some(x),
            _ => Some(self.n),
        };
        (lower, upper)
    }
}

impl<I: DoubleEndedIterator + ExactSizeIterator> DoubleEndedIterator for Take<I> {
    fn next_back(&mut self) -> Option<I::Item> {
        if self.n == 0 {
            None
        } else {
            let n = self.n;
            self.n -= 1;
            let skip = self.iter.len().saturating_sub(n);
            self.iter.nth_back(skip)
        }
    }
}

impl<I: ExactSizeIterator> ExactSizeIterator for Take<I> {}

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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.iter.size_hint();
        let lower = lower.saturating_sub(self.n);
        let upper = match upper {
            Some(x) => Some(x.saturating_sub(self.n)),
            None => None,
        };
        (lower, upper)
    }
}

impl<I: DoubleEndedIterator + ExactSizeIterator> DoubleEndedIterator for Skip<I> {
    fn next_back(&mut self) -> Option<I::Item> {
        if self.len() > 0 { self.iter.next_back() } else { None }
    }
}

impl<I: ExactSizeIterator> ExactSizeIterator for Skip<I> {}

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

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.flag {
            (0, Some(0))
        } else {
            let (_, upper) = self.iter.size_hint();
            (0, upper)
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper)
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (low, high) = self.iter.size_hint();
        let step = self.step + 1;
        if self.first_take {
            let f = move |n: usize| if n == 0 { 0 } else { 1 + (n - 1) / step };
            (f(low), high.map(f))
        } else {
            let f = move |n: usize| n / step;
            (f(low), high.map(f))
        }
    }
}

impl<I: ExactSizeIterator> ExactSizeIterator for StepBy<I> {}

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

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for Rev<I> {
    fn next_back(&mut self) -> Option<I::Item> {
        self.iter.next()
    }
}

impl<I: DoubleEndedIterator + ExactSizeIterator> ExactSizeIterator for Rev<I> {}

impl<B, I: ExactSizeIterator, F: FnMut(I::Item) -> B> ExactSizeIterator for Map<I, F> {}

/// [`Iterator::copied`]'s.
#[derive(Clone)]
pub struct Copied<I> {
    it: I,
}

impl<'a, T: Copy, I: Iterator<Item = &'a T>> Iterator for Copied<I> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        match self.it.next() {
            Some(&x) => Some(x),
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.it.size_hint()
    }
}

impl<'a, T: Copy, I: DoubleEndedIterator<Item = &'a T>> DoubleEndedIterator for Copied<I> {
    fn next_back(&mut self) -> Option<T> {
        match self.it.next_back() {
            Some(&x) => Some(x),
            None => None,
        }
    }
}

impl<'a, T: Copy, I: ExactSizeIterator<Item = &'a T>> ExactSizeIterator for Copied<I> {}

/// [`Iterator::cloned`]'s.
#[derive(Clone)]
pub struct Cloned<I> {
    it: I,
}

impl<'a, T: Clone, I: Iterator<Item = &'a T>> Iterator for Cloned<I> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        match self.it.next() {
            Some(x) => Some(x.clone()),
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.it.size_hint()
    }
}

impl<'a, T: Clone, I: DoubleEndedIterator<Item = &'a T>> DoubleEndedIterator for Cloned<I> {
    fn next_back(&mut self) -> Option<T> {
        match self.it.next_back() {
            Some(x) => Some(x.clone()),
            None => None,
        }
    }
}

impl<'a, T: Clone, I: ExactSizeIterator<Item = &'a T>> ExactSizeIterator for Cloned<I> {}

/// [`Iterator::fuse`]'s.
#[derive(Clone)]
pub struct Fuse<I> {
    iter: Option<I>,
}

impl<I: Iterator> Iterator for Fuse<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        match &mut self.iter {
            Some(iter) => {
                let x = iter.next();
                if x.is_none() {
                    self.iter = None;
                }
                x
            }
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.iter {
            Some(iter) => iter.size_hint(),
            None => (0, Some(0)),
        }
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for Fuse<I> {
    fn next_back(&mut self) -> Option<I::Item> {
        match &mut self.iter {
            Some(iter) => {
                let x = iter.next_back();
                if x.is_none() {
                    self.iter = None;
                }
                x
            }
            None => None,
        }
    }
}

impl<I: ExactSizeIterator> ExactSizeIterator for Fuse<I> {}

/// [`Iterator::inspect`]'s.
#[derive(Clone)]
pub struct Inspect<I, F> {
    iter: I,
    f: F,
}

impl<I: Iterator, F: FnMut(&I::Item)> Iterator for Inspect<I, F> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        let next = self.iter.next();
        if let Some(a) = &next {
            (self.f)(a);
        }
        next
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<I: DoubleEndedIterator, F: FnMut(&I::Item)> DoubleEndedIterator for Inspect<I, F> {
    fn next_back(&mut self) -> Option<I::Item> {
        let next = self.iter.next_back();
        if let Some(a) = &next {
            (self.f)(a);
        }
        next
    }
}

impl<I: ExactSizeIterator, F: FnMut(&I::Item)> ExactSizeIterator for Inspect<I, F> {}

/// [`Iterator::scan`]'s.
#[derive(Clone)]
pub struct Scan<I, St, F> {
    iter: I,
    f: F,
    state: St,
}

impl<B, I: Iterator, St, F: FnMut(&mut St, I::Item) -> Option<B>> Iterator for Scan<I, St, F> {
    type Item = B;

    fn next(&mut self) -> Option<B> {
        let a = self.iter.next()?;
        (self.f)(&mut self.state, a)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper)
    }
}

/// [`Iterator::map_while`]'s.
#[derive(Clone)]
pub struct MapWhile<I, P> {
    iter: I,
    predicate: P,
}

impl<B, I: Iterator, P: FnMut(I::Item) -> Option<B>> Iterator for MapWhile<I, P> {
    type Item = B;

    fn next(&mut self) -> Option<B> {
        let x = self.iter.next()?;
        (self.predicate)(x)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.iter.size_hint();
        (0, upper)
    }
}

/// [`Iterator::cycle`]'s.
#[derive(Clone)]
pub struct Cycle<I> {
    orig: I,
    iter: I,
}

impl<I: Clone + Iterator> Iterator for Cycle<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        match self.iter.next() {
            None => {
                self.iter = self.orig.clone();
                self.iter.next()
            }
            y => y,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.orig.size_hint() {
            (0, Some(0)) => (0, Some(0)),
            (0, _) => (0, None),
            _ => (usize::MAX, None),
        }
    }
}

/// [`Iterator::peekable`]'s.
pub struct Peekable<I: Iterator> {
    iter: I,
    /// The item `peek` took from `iter`, or `None` if it took none.
    peeked: Option<Option<I::Item>>,
}

impl<I: Iterator + Clone> Clone for Peekable<I>
where
    I::Item: Clone,
{
    fn clone(&self) -> Self {
        Peekable {
            iter: self.iter.clone(),
            peeked: self.peeked.clone(),
        }
    }
}

impl<I: Iterator> Iterator for Peekable<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        match self.peeked.take() {
            Some(v) => v,
            None => self.iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let peek_len = match &self.peeked {
            Some(None) => return (0, Some(0)),
            Some(Some(_)) => 1,
            None => 0,
        };
        let (lower, upper) = self.iter.size_hint();
        let lower = lower.saturating_add(peek_len);
        let upper = match upper {
            Some(x) => x.checked_add(peek_len),
            None => None,
        };
        (lower, upper)
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for Peekable<I> {
    fn next_back(&mut self) -> Option<I::Item> {
        match &self.peeked {
            Some(None) => None,
            None => self.iter.next_back(),
            Some(Some(_)) => match self.iter.next_back() {
                Some(x) => Some(x),
                None => match self.peeked.take() {
                    Some(v) => v,
                    None => None,
                },
            },
        }
    }
}

impl<I: ExactSizeIterator> ExactSizeIterator for Peekable<I> {}

impl<I: Iterator> Peekable<I> {
    /// A reference to the next item, without taking it.
    pub fn peek(&mut self) -> Option<&I::Item> {
        if let None = self.peeked {
            self.peeked = Some(self.iter.next());
        }
        match &self.peeked {
            Some(Some(value)) => Some(value),
            _ => None,
        }
    }

    /// A mutable reference to the next item, without taking it.
    pub fn peek_mut(&mut self) -> Option<&mut I::Item> {
        if let None = self.peeked {
            self.peeked = Some(self.iter.next());
        }
        match &mut self.peeked {
            Some(Some(value)) => Some(value),
            _ => None,
        }
    }

    /// The next item, if `func` is `true` for it; otherwise it stays.
    pub fn next_if<F: FnOnce(&I::Item) -> bool>(&mut self, func: F) -> Option<I::Item> {
        let next = self.next();
        if let Some(matched) = &next
            && func(matched)
        {
            return next;
        }
        self.peeked = Some(next);
        None
    }

    /// The next item, if it equals `expected`; otherwise it stays.
    pub fn next_if_eq<T>(&mut self, expected: &T) -> Option<I::Item>
    where
        I::Item: PartialEq<T>,
    {
        self.next_if(|next| next == expected)
    }
}

/// [`Iterator::flat_map`]'s.
pub struct FlatMap<I, U: IntoIterator, F> {
    iter: Fuse<Map<I, F>>,
    frontiter: Option<U::IntoIter>,
    backiter: Option<U::IntoIter>,
}

impl<I: Clone, U: IntoIterator, F: Clone> Clone for FlatMap<I, U, F>
where
    U::IntoIter: Clone,
{
    fn clone(&self) -> Self {
        FlatMap {
            iter: self.iter.clone(),
            frontiter: self.frontiter.clone(),
            backiter: self.backiter.clone(),
        }
    }
}

impl<I: Iterator, U: IntoIterator, F: FnMut(I::Item) -> U> Iterator for FlatMap<I, U, F> {
    type Item = U::Item;

    fn next(&mut self) -> Option<U::Item> {
        loop {
            if let Some(inner) = &mut self.frontiter {
                match inner.next() {
                    Some(x) => return Some(x),
                    None => self.frontiter = None,
                }
            }
            match self.iter.next() {
                Some(inner) => self.frontiter = Some(inner.into_iter()),
                None => {
                    return match &mut self.backiter {
                        Some(back) => {
                            let x = back.next();
                            if x.is_none() {
                                self.backiter = None;
                            }
                            x
                        }
                        None => None,
                    };
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (flo, fhi) = match &self.frontiter {
            Some(it) => it.size_hint(),
            None => (0, Some(0)),
        };
        let (blo, bhi) = match &self.backiter {
            Some(it) => it.size_hint(),
            None => (0, Some(0)),
        };
        let lo = flo.saturating_add(blo);
        match (self.iter.size_hint(), fhi, bhi) {
            ((0, Some(0)), Some(a), Some(b)) => (lo, a.checked_add(b)),
            _ => (lo, None),
        }
    }
}

impl<I: DoubleEndedIterator, U: IntoIterator, F: FnMut(I::Item) -> U> DoubleEndedIterator
    for FlatMap<I, U, F>
where
    U::IntoIter: DoubleEndedIterator,
{
    fn next_back(&mut self) -> Option<U::Item> {
        loop {
            if let Some(inner) = &mut self.backiter {
                match inner.next_back() {
                    Some(x) => return Some(x),
                    None => self.backiter = None,
                }
            }
            match self.iter.next_back() {
                Some(inner) => self.backiter = Some(inner.into_iter()),
                None => {
                    return match &mut self.frontiter {
                        Some(front) => {
                            let x = front.next_back();
                            if x.is_none() {
                                self.frontiter = None;
                            }
                            x
                        }
                        None => None,
                    };
                }
            }
        }
    }
}

/// [`Iterator::flatten`]'s.
pub struct Flatten<I>
where
    I: Iterator,
    I::Item: IntoIterator,
{
    iter: Fuse<I>,
    frontiter: Option<<I::Item as IntoIterator>::IntoIter>,
    backiter: Option<<I::Item as IntoIterator>::IntoIter>,
}

impl<I: Iterator + Clone> Clone for Flatten<I>
where
    I::Item: IntoIterator,
    <I::Item as IntoIterator>::IntoIter: Clone,
{
    fn clone(&self) -> Self {
        Flatten {
            iter: self.iter.clone(),
            frontiter: self.frontiter.clone(),
            backiter: self.backiter.clone(),
        }
    }
}

impl<I: Iterator> Iterator for Flatten<I>
where
    I::Item: IntoIterator,
{
    type Item = <I::Item as IntoIterator>::Item;

    fn next(&mut self) -> Option<<I::Item as IntoIterator>::Item> {
        loop {
            if let Some(inner) = &mut self.frontiter {
                match inner.next() {
                    Some(x) => return Some(x),
                    None => self.frontiter = None,
                }
            }
            match self.iter.next() {
                Some(inner) => self.frontiter = Some(inner.into_iter()),
                None => {
                    return match &mut self.backiter {
                        Some(back) => {
                            let x = back.next();
                            if x.is_none() {
                                self.backiter = None;
                            }
                            x
                        }
                        None => None,
                    };
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (flo, fhi) = match &self.frontiter {
            Some(it) => it.size_hint(),
            None => (0, Some(0)),
        };
        let (blo, bhi) = match &self.backiter {
            Some(it) => it.size_hint(),
            None => (0, Some(0)),
        };
        let lo = flo.saturating_add(blo);
        match (self.iter.size_hint(), fhi, bhi) {
            ((0, Some(0)), Some(a), Some(b)) => (lo, a.checked_add(b)),
            _ => (lo, None),
        }
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for Flatten<I>
where
    I::Item: IntoIterator,
    <I::Item as IntoIterator>::IntoIter: DoubleEndedIterator,
{
    fn next_back(&mut self) -> Option<<I::Item as IntoIterator>::Item> {
        loop {
            if let Some(inner) = &mut self.backiter {
                match inner.next_back() {
                    Some(x) => return Some(x),
                    None => self.backiter = None,
                }
            }
            match self.iter.next_back() {
                Some(inner) => self.backiter = Some(inner.into_iter()),
                None => {
                    return match &mut self.frontiter {
                        Some(front) => {
                            let x = front.next_back();
                            if x.is_none() {
                                self.frontiter = None;
                            }
                            x
                        }
                        None => None,
                    };
                }
            }
        }
    }
}

// The integer types step, add up and multiply, written out for each
// since flux has no macros. As in rustc, the ranges of the integers
// up to 32 bits (and of `usize` and `isize`) know their length, and the
// inclusive ones up to 16 bits.
impl Step for i8 { fn forward_one(self) -> i8 { self.wrapping_add(1) } fn backward_one(self) -> i8 { self.wrapping_sub(1) } fn steps_between(start: &i8, end: &i8) -> (usize, Option<usize>) { if *start <= *end { let n = (*end as isize).wrapping_sub(*start as isize) as usize; (n, Some(n)) } else { (0, None) } } }
impl Sum for i8 { fn sum<I: Iterator<Item = i8>>(mut iter: I) -> i8 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a i8> for i8 { fn sum<I: Iterator<Item = &'a i8>>(mut iter: I) -> i8 { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for i8 { fn product<I: Iterator<Item = i8>>(mut iter: I) -> i8 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a i8> for i8 { fn product<I: Iterator<Item = &'a i8>>(mut iter: I) -> i8 { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl ExactSizeIterator for Range<i8> {}
impl ExactSizeIterator for RangeInclusive<i8> {}
impl Step for i16 { fn forward_one(self) -> i16 { self.wrapping_add(1) } fn backward_one(self) -> i16 { self.wrapping_sub(1) } fn steps_between(start: &i16, end: &i16) -> (usize, Option<usize>) { if *start <= *end { let n = (*end as isize).wrapping_sub(*start as isize) as usize; (n, Some(n)) } else { (0, None) } } }
impl Sum for i16 { fn sum<I: Iterator<Item = i16>>(mut iter: I) -> i16 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a i16> for i16 { fn sum<I: Iterator<Item = &'a i16>>(mut iter: I) -> i16 { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for i16 { fn product<I: Iterator<Item = i16>>(mut iter: I) -> i16 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a i16> for i16 { fn product<I: Iterator<Item = &'a i16>>(mut iter: I) -> i16 { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl ExactSizeIterator for Range<i16> {}
impl ExactSizeIterator for RangeInclusive<i16> {}
impl Step for i32 { fn forward_one(self) -> i32 { self.wrapping_add(1) } fn backward_one(self) -> i32 { self.wrapping_sub(1) } fn steps_between(start: &i32, end: &i32) -> (usize, Option<usize>) { if *start <= *end { let n = (*end as isize).wrapping_sub(*start as isize) as usize; (n, Some(n)) } else { (0, None) } } }
impl Sum for i32 { fn sum<I: Iterator<Item = i32>>(mut iter: I) -> i32 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a i32> for i32 { fn sum<I: Iterator<Item = &'a i32>>(mut iter: I) -> i32 { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for i32 { fn product<I: Iterator<Item = i32>>(mut iter: I) -> i32 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a i32> for i32 { fn product<I: Iterator<Item = &'a i32>>(mut iter: I) -> i32 { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl ExactSizeIterator for Range<i32> {}
impl Step for i64 { fn forward_one(self) -> i64 { self.wrapping_add(1) } fn backward_one(self) -> i64 { self.wrapping_sub(1) } fn steps_between(start: &i64, end: &i64) -> (usize, Option<usize>) { if *start <= *end { let n = (*end as isize).wrapping_sub(*start as isize) as usize; (n, Some(n)) } else { (0, None) } } }
impl Sum for i64 { fn sum<I: Iterator<Item = i64>>(mut iter: I) -> i64 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a i64> for i64 { fn sum<I: Iterator<Item = &'a i64>>(mut iter: I) -> i64 { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for i64 { fn product<I: Iterator<Item = i64>>(mut iter: I) -> i64 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a i64> for i64 { fn product<I: Iterator<Item = &'a i64>>(mut iter: I) -> i64 { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl Step for i128 { fn forward_one(self) -> i128 { self.wrapping_add(1) } fn backward_one(self) -> i128 { self.wrapping_sub(1) } fn steps_between(start: &i128, end: &i128) -> (usize, Option<usize>) { if *start <= *end { let n = end.wrapping_sub(*start) as u128; if n <= usize::MAX as u128 { (n as usize, Some(n as usize)) } else { (usize::MAX, None) } } else { (0, None) } } }
impl Sum for i128 { fn sum<I: Iterator<Item = i128>>(mut iter: I) -> i128 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a i128> for i128 { fn sum<I: Iterator<Item = &'a i128>>(mut iter: I) -> i128 { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for i128 { fn product<I: Iterator<Item = i128>>(mut iter: I) -> i128 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a i128> for i128 { fn product<I: Iterator<Item = &'a i128>>(mut iter: I) -> i128 { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl Step for isize { fn forward_one(self) -> isize { self.wrapping_add(1) } fn backward_one(self) -> isize { self.wrapping_sub(1) } fn steps_between(start: &isize, end: &isize) -> (usize, Option<usize>) { if *start <= *end { let n = (*end as isize).wrapping_sub(*start as isize) as usize; (n, Some(n)) } else { (0, None) } } }
impl Sum for isize { fn sum<I: Iterator<Item = isize>>(mut iter: I) -> isize { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a isize> for isize { fn sum<I: Iterator<Item = &'a isize>>(mut iter: I) -> isize { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for isize { fn product<I: Iterator<Item = isize>>(mut iter: I) -> isize { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a isize> for isize { fn product<I: Iterator<Item = &'a isize>>(mut iter: I) -> isize { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl ExactSizeIterator for Range<isize> {}
impl Step for u8 { fn forward_one(self) -> u8 { self.wrapping_add(1) } fn backward_one(self) -> u8 { self.wrapping_sub(1) } fn steps_between(start: &u8, end: &u8) -> (usize, Option<usize>) { if *start <= *end { let n = (*end - *start) as usize; (n, Some(n)) } else { (0, None) } } }
impl Sum for u8 { fn sum<I: Iterator<Item = u8>>(mut iter: I) -> u8 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a u8> for u8 { fn sum<I: Iterator<Item = &'a u8>>(mut iter: I) -> u8 { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for u8 { fn product<I: Iterator<Item = u8>>(mut iter: I) -> u8 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a u8> for u8 { fn product<I: Iterator<Item = &'a u8>>(mut iter: I) -> u8 { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl ExactSizeIterator for Range<u8> {}
impl ExactSizeIterator for RangeInclusive<u8> {}
impl Step for u16 { fn forward_one(self) -> u16 { self.wrapping_add(1) } fn backward_one(self) -> u16 { self.wrapping_sub(1) } fn steps_between(start: &u16, end: &u16) -> (usize, Option<usize>) { if *start <= *end { let n = (*end - *start) as usize; (n, Some(n)) } else { (0, None) } } }
impl Sum for u16 { fn sum<I: Iterator<Item = u16>>(mut iter: I) -> u16 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a u16> for u16 { fn sum<I: Iterator<Item = &'a u16>>(mut iter: I) -> u16 { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for u16 { fn product<I: Iterator<Item = u16>>(mut iter: I) -> u16 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a u16> for u16 { fn product<I: Iterator<Item = &'a u16>>(mut iter: I) -> u16 { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl ExactSizeIterator for Range<u16> {}
impl ExactSizeIterator for RangeInclusive<u16> {}
impl Step for u32 { fn forward_one(self) -> u32 { self.wrapping_add(1) } fn backward_one(self) -> u32 { self.wrapping_sub(1) } fn steps_between(start: &u32, end: &u32) -> (usize, Option<usize>) { if *start <= *end { let n = (*end - *start) as usize; (n, Some(n)) } else { (0, None) } } }
impl Sum for u32 { fn sum<I: Iterator<Item = u32>>(mut iter: I) -> u32 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a u32> for u32 { fn sum<I: Iterator<Item = &'a u32>>(mut iter: I) -> u32 { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for u32 { fn product<I: Iterator<Item = u32>>(mut iter: I) -> u32 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a u32> for u32 { fn product<I: Iterator<Item = &'a u32>>(mut iter: I) -> u32 { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl ExactSizeIterator for Range<u32> {}
impl Step for u64 { fn forward_one(self) -> u64 { self.wrapping_add(1) } fn backward_one(self) -> u64 { self.wrapping_sub(1) } fn steps_between(start: &u64, end: &u64) -> (usize, Option<usize>) { if *start <= *end { let n = (*end - *start) as usize; (n, Some(n)) } else { (0, None) } } }
impl Sum for u64 { fn sum<I: Iterator<Item = u64>>(mut iter: I) -> u64 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a u64> for u64 { fn sum<I: Iterator<Item = &'a u64>>(mut iter: I) -> u64 { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for u64 { fn product<I: Iterator<Item = u64>>(mut iter: I) -> u64 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a u64> for u64 { fn product<I: Iterator<Item = &'a u64>>(mut iter: I) -> u64 { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl Step for u128 { fn forward_one(self) -> u128 { self.wrapping_add(1) } fn backward_one(self) -> u128 { self.wrapping_sub(1) } fn steps_between(start: &u128, end: &u128) -> (usize, Option<usize>) { if *start <= *end { let n = *end - *start; if n <= usize::MAX as u128 { (n as usize, Some(n as usize)) } else { (usize::MAX, None) } } else { (0, None) } } }
impl Sum for u128 { fn sum<I: Iterator<Item = u128>>(mut iter: I) -> u128 { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a u128> for u128 { fn sum<I: Iterator<Item = &'a u128>>(mut iter: I) -> u128 { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for u128 { fn product<I: Iterator<Item = u128>>(mut iter: I) -> u128 { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a u128> for u128 { fn product<I: Iterator<Item = &'a u128>>(mut iter: I) -> u128 { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl Step for usize { fn forward_one(self) -> usize { self.wrapping_add(1) } fn backward_one(self) -> usize { self.wrapping_sub(1) } fn steps_between(start: &usize, end: &usize) -> (usize, Option<usize>) { if *start <= *end { let n = (*end - *start) as usize; (n, Some(n)) } else { (0, None) } } }
impl Sum for usize { fn sum<I: Iterator<Item = usize>>(mut iter: I) -> usize { let mut total = 0; while let Some(x) = iter.next() { total += x; } total } }
impl<'a> Sum<&'a usize> for usize { fn sum<I: Iterator<Item = &'a usize>>(mut iter: I) -> usize { let mut total = 0; while let Some(x) = iter.next() { total += *x; } total } }
impl Product for usize { fn product<I: Iterator<Item = usize>>(mut iter: I) -> usize { let mut total = 1; while let Some(x) = iter.next() { total *= x; } total } }
impl<'a> Product<&'a usize> for usize { fn product<I: Iterator<Item = &'a usize>>(mut iter: I) -> usize { let mut total = 1; while let Some(x) = iter.next() { total *= *x; } total } }
impl ExactSizeIterator for Range<usize> {}

/// An iterator with no items: what [`empty`] gives.
pub struct Empty<T> {
    marker: PhantomData<T>,
}

/// An iterator with no items.
pub const fn empty<T>() -> Empty<T> {
    Empty { marker: PhantomData }
}

impl<T> Iterator for Empty<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }
}

impl<T> DoubleEndedIterator for Empty<T> {
    fn next_back(&mut self) -> Option<T> {
        None
    }
}

impl<T> ExactSizeIterator for Empty<T> {
    fn len(&self) -> usize {
        0
    }
}

impl<T> crate::fmt::Debug for Empty<T> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.write_str("Empty")
    }
}

impl<T> Clone for Empty<T> {
    fn clone(&self) -> Empty<T> {
        Empty { marker: PhantomData }
    }
}

impl<T> crate::default::Default for Empty<T> {
    fn default() -> Empty<T> {
        Empty { marker: PhantomData }
    }
}

/// An iterator with one item: what [`once`] gives.
pub struct Once<T> {
    inner: Option<T>,
}

/// An iterator that gives `value` once.
pub fn once<T>(value: T) -> Once<T> {
    Once { inner: Some(value) }
}

impl<T> Iterator for Once<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.inner.take()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = if self.inner.is_some() { 1 } else { 0 };
        (n, Some(n))
    }
}

impl<T> DoubleEndedIterator for Once<T> {
    fn next_back(&mut self) -> Option<T> {
        self.inner.take()
    }
}

impl<T> ExactSizeIterator for Once<T> {}

impl<T: Clone> Clone for Once<T> {
    fn clone(&self) -> Once<T> {
        Once { inner: self.inner.clone() }
    }
}

/// An iterator that gives `f()` once: what [`once_with`] gives.
pub struct OnceWith<F> {
    make: Option<F>,
}

/// An iterator that gives `f()` once, calling it when asked.
pub fn once_with<A, F: FnOnce() -> A>(make: F) -> OnceWith<F> {
    OnceWith { make: Some(make) }
}

impl<A, F: FnOnce() -> A> Iterator for OnceWith<F> {
    type Item = A;

    fn next(&mut self) -> Option<A> {
        match self.make.take() {
            Some(f) => Some(f()),
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = if self.make.is_some() { 1 } else { 0 };
        (n, Some(n))
    }
}

impl<A, F: FnOnce() -> A> DoubleEndedIterator for OnceWith<F> {
    fn next_back(&mut self) -> Option<A> {
        self.next()
    }
}

impl<A, F: FnOnce() -> A> ExactSizeIterator for OnceWith<F> {}

/// An iterator that gives clones of one value forever: what [`repeat`]
/// gives.
pub struct Repeat<A> {
    element: A,
}

/// An iterator that gives clones of `elt` forever.
pub fn repeat<T: Clone>(elt: T) -> Repeat<T> {
    Repeat { element: elt }
}

impl<A: Clone> Iterator for Repeat<A> {
    type Item = A;

    fn next(&mut self) -> Option<A> {
        Some(self.element.clone())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::MAX, None)
    }

    fn nth(&mut self, n: usize) -> Option<A> {
        let _ = n;
        Some(self.element.clone())
    }
}

impl<A: Clone> DoubleEndedIterator for Repeat<A> {
    fn next_back(&mut self) -> Option<A> {
        Some(self.element.clone())
    }

    fn nth_back(&mut self, n: usize) -> Option<A> {
        let _ = n;
        Some(self.element.clone())
    }
}

impl<A: crate::fmt::Debug> crate::fmt::Debug for Repeat<A> {
    fn fmt(&self, f: &mut crate::fmt::Formatter<'_>) -> crate::fmt::Result {
        f.debug_struct("Repeat").field("element", &self.element).finish()
    }
}

impl<A: Clone> Clone for Repeat<A> {
    fn clone(&self) -> Repeat<A> {
        Repeat { element: self.element.clone() }
    }
}

/// An iterator that gives `f()` forever: what [`repeat_with`] gives.
pub struct RepeatWith<F> {
    repeater: F,
}

/// An iterator that calls `repeater` for each item, forever.
pub fn repeat_with<A, F: FnMut() -> A>(repeater: F) -> RepeatWith<F> {
    RepeatWith { repeater }
}

impl<A, F: FnMut() -> A> Iterator for RepeatWith<F> {
    type Item = A;

    fn next(&mut self) -> Option<A> {
        Some((self.repeater)())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::MAX, None)
    }
}

impl<F: Clone> Clone for RepeatWith<F> {
    fn clone(&self) -> RepeatWith<F> {
        RepeatWith { repeater: self.repeater.clone() }
    }
}

/// An iterator that gives a value `count` times: clones of it, then the
/// value itself. What [`repeat_n`] gives.
pub struct RepeatN<A> {
    /// `None` once the count is 0.
    element: Option<A>,
    count: usize,
}

/// An iterator that gives `element` `count` times: clones of it, then
/// the value itself last. With `count` 0, `element` is dropped at once.
pub fn repeat_n<T: Clone>(element: T, count: usize) -> RepeatN<T> {
    if count == 0 {
        crate::mem::drop(element);
        RepeatN { element: None, count: 0 }
    } else {
        RepeatN { element: Some(element), count }
    }
}

impl<A: Clone> Iterator for RepeatN<A> {
    type Item = A;

    fn next(&mut self) -> Option<A> {
        if self.count == 0 {
            return None;
        }
        self.count -= 1;
        if self.count == 0 {
            self.element.take()
        } else {
            match &self.element {
                Some(e) => Some(e.clone()),
                None => None,
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.count, Some(self.count))
    }
}

impl<A: Clone> DoubleEndedIterator for RepeatN<A> {
    fn next_back(&mut self) -> Option<A> {
        self.next()
    }
}

impl<A: Clone> ExactSizeIterator for RepeatN<A> {
    fn len(&self) -> usize {
        self.count
    }
}

impl<A: Clone> Clone for RepeatN<A> {
    fn clone(&self) -> RepeatN<A> {
        RepeatN { element: self.element.clone(), count: self.count }
    }
}

/// An iterator whose items `succ` makes, each from the one before:
/// what [`successors`] gives.
pub struct Successors<T, F> {
    next: Option<T>,
    succ: F,
}

/// An iterator that gives `first`, then `succ` of each item, until
/// either is `None`.
pub fn successors<T, F: FnMut(&T) -> Option<T>>(first: Option<T>, succ: F) -> Successors<T, F> {
    Successors { next: first, succ }
}

impl<T, F: FnMut(&T) -> Option<T>> Iterator for Successors<T, F> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        let item = self.next.take()?;
        self.next = (self.succ)(&item);
        Some(item)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.next.is_some() { (1, None) } else { (0, Some(0)) }
    }
}

impl<T: Clone, F: Clone> Clone for Successors<T, F> {
    fn clone(&self) -> Successors<T, F> {
        Successors { next: self.next.clone(), succ: self.succ.clone() }
    }
}

/// An iterator whose items are `f()`'s, until it gives `None`: what
/// [`from_fn`] gives.
pub struct FromFn<F> {
    f: F,
}

/// An iterator that calls `f` for each item, giving what it returns.
pub fn from_fn<T, F: FnMut() -> Option<T>>(f: F) -> FromFn<F> {
    FromFn { f }
}

impl<T, F: FnMut() -> Option<T>> Iterator for FromFn<F> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        (self.f)()
    }
}

impl<F: Clone> Clone for FromFn<F> {
    fn clone(&self) -> FromFn<F> {
        FromFn { f: self.f.clone() }
    }
}

/// A type built from an iterator's items: what `collect` makes.
#[diagnostic::on_unimplemented(
    message = "a value of type `{Self}` cannot be built from an iterator over elements of type `{A}`"
)]
pub trait FromIterator<A>: Sized {
    /// The value of the items of `iter`.
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self;
}

/// A collection that takes the items of an iterator.
pub trait Extend<A> {
    /// Adds the items of `iter`.
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T);
}

impl FromIterator<()> for () {
    fn from_iter<T: IntoIterator<Item = ()>>(iter: T) {
        let mut iter = iter.into_iter();
        while let Some(()) = iter.next() {}
    }
}

impl Extend<()> for () {
    fn extend<T: IntoIterator<Item = ()>>(&mut self, iter: T) {
        let mut iter = iter.into_iter();
        while let Some(()) = iter.next() {}
    }
}

/// Each half of the pairs into each collection, one pair at a time.
impl<A, B, ExtendA: Extend<A>, ExtendB: Extend<B>> Extend<(A, B)> for (ExtendA, ExtendB) {
    fn extend<T: IntoIterator<Item = (A, B)>>(&mut self, into_iter: T) {
        let mut iter = into_iter.into_iter();
        while let Some((a, b)) = iter.next() {
            self.0.extend(Some(a));
            self.1.extend(Some(b));
        }
    }
}

impl<A, B, AE: crate::default::Default + Extend<A>, BE: crate::default::Default + Extend<B>>
    FromIterator<(A, B)> for (AE, BE)
{
    fn from_iter<T: IntoIterator<Item = (A, B)>>(iter: T) -> (AE, BE) {
        let mut res: (AE, BE) = (AE::default(), BE::default());
        res.extend(iter);
        res
    }
}

/// The values of an iterator of `Option`s, up to the first `None`, which
/// it records: rustc's `GenericShunt`.
struct OptionShunt<'a, I> {
    iter: I,
    failed: &'a mut bool,
}

impl<'a, A, I: Iterator<Item = Option<A>>> Iterator for OptionShunt<'a, I> {
    type Item = A;

    fn next(&mut self) -> Option<A> {
        if *self.failed {
            return None;
        }
        match self.iter.next() {
            Some(Some(a)) => Some(a),
            Some(None) => {
                *self.failed = true;
                None
            }
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if *self.failed {
            (0, Some(0))
        } else {
            (0, self.iter.size_hint().1)
        }
    }
}

/// The values of an iterator of `Result`s, up to the first `Err`, whose
/// error it keeps.
struct ResultShunt<'a, I, E> {
    iter: I,
    error: &'a mut Option<E>,
}

impl<'a, A, E, I: Iterator<Item = Result<A, E>>> Iterator for ResultShunt<'a, I, E> {
    type Item = A;

    fn next(&mut self) -> Option<A> {
        if self.error.is_some() {
            return None;
        }
        match self.iter.next() {
            Some(Ok(a)) => Some(a),
            Some(Err(e)) => {
                *self.error = Some(e);
                None
            }
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.error.is_some() {
            (0, Some(0))
        } else {
            (0, self.iter.size_hint().1)
        }
    }
}

/// `Some` of the collection of the values, or `None` if an item is
/// (stopping there).
impl<A, V: FromIterator<A>> FromIterator<Option<A>> for Option<V> {
    fn from_iter<I: IntoIterator<Item = Option<A>>>(iter: I) -> Option<V> {
        let mut failed = false;
        let value: V = V::from_iter(OptionShunt { iter: iter.into_iter(), failed: &mut failed });
        if failed { None } else { Some(value) }
    }
}

/// `Ok` of the collection of the values, or the first `Err` (stopping
/// there).
impl<A, E, V: FromIterator<A>> FromIterator<Result<A, E>> for Result<V, E> {
    fn from_iter<I: IntoIterator<Item = Result<A, E>>>(iter: I) -> Result<V, E> {
        let mut error = None;
        let value: V = V::from_iter(ResultShunt { iter: iter.into_iter(), error: &mut error });
        match error {
            Some(e) => Err(e),
            None => Ok(value),
        }
    }
}

/// The sum of the values, or `None` if an item is (stopping there).
impl<T, U: Sum<T>> Sum<Option<T>> for Option<U> {
    fn sum<I: Iterator<Item = Option<T>>>(iter: I) -> Option<U> {
        let mut failed = false;
        let value: U = U::sum(OptionShunt { iter, failed: &mut failed });
        if failed { None } else { Some(value) }
    }
}

/// The product of the values, or `None` if an item is (stopping there).
impl<T, U: Product<T>> Product<Option<T>> for Option<U> {
    fn product<I: Iterator<Item = Option<T>>>(iter: I) -> Option<U> {
        let mut failed = false;
        let value: U = U::product(OptionShunt { iter, failed: &mut failed });
        if failed { None } else { Some(value) }
    }
}

/// The sum of the values, or the first `Err` (stopping there).
impl<T, U: Sum<T>, E> Sum<Result<T, E>> for Result<U, E> {
    fn sum<I: Iterator<Item = Result<T, E>>>(iter: I) -> Result<U, E> {
        let mut error = None;
        let value: U = U::sum(ResultShunt { iter, error: &mut error });
        match error {
            Some(e) => Err(e),
            None => Ok(value),
        }
    }
}

/// The product of the values, or the first `Err` (stopping there).
impl<T, U: Product<T>, E> Product<Result<T, E>> for Result<U, E> {
    fn product<I: Iterator<Item = Result<T, E>>>(iter: I) -> Result<U, E> {
        let mut error = None;
        let value: U = U::product(ResultShunt { iter, error: &mut error });
        match error {
            Some(e) => Err(e),
            None => Ok(value),
        }
    }
}
