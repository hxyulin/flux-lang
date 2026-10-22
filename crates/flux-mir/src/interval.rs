//! Sets of points as sorted, disjoint half-open intervals: a region's
//! points are mostly runs of consecutive statements, so this stays small
//! where a bit per point would make every region as big as its body.

/// A set of `u32`s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IntervalSet {
    /// `[start, end)`, sorted, disjoint and not adjacent.
    runs: Vec<(u32, u32)>,
}

impl IntervalSet {
    pub fn new() -> IntervalSet {
        IntervalSet::default()
    }

    /// Every point below `end`.
    pub fn full(end: u32) -> IntervalSet {
        let mut set = IntervalSet::new();
        if end > 0 {
            set.runs.push((0, end));
        }
        set
    }

    #[cfg(test)]
    pub fn runs(&self) -> &[(u32, u32)] {
        &self.runs
    }

    /// Adds `[start, end)`, which must not start before the last run's
    /// start: the order intervals come in while building.
    pub fn push(&mut self, start: u32, end: u32) {
        if start >= end {
            return;
        }
        if let Some(last) = self.runs.last_mut()
            && start <= last.1
        {
            debug_assert!(start >= last.0, "runs are pushed in order");
            last.1 = last.1.max(end);
            return;
        }
        self.runs.push((start, end));
    }

    /// Adds `[start, end)` anywhere.
    #[cfg(test)]
    pub fn insert(&mut self, start: u32, end: u32) {
        if start >= end {
            return;
        }
        match self.runs.last() {
            None => self.runs.push((start, end)),
            Some(&(s, _)) if start >= s => self.push(start, end),
            _ => {
                let other = IntervalSet {
                    runs: vec![(start, end)],
                };
                self.union(&other);
            }
        }
    }

    pub fn contains(&self, point: u32) -> bool {
        self.run_of(point).is_some()
    }

    /// The run containing `point`.
    pub fn run_of(&self, point: u32) -> Option<(u32, u32)> {
        let i = self.runs.partition_point(|&(_, end)| end <= point);
        self.runs
            .get(i)
            .copied()
            .filter(|&(start, _)| start <= point)
    }

    /// Adds `other`'s points; whether that added any.
    pub fn union(&mut self, other: &IntervalSet) -> bool {
        if other.runs.is_empty() {
            return false;
        }
        if self.runs.is_empty() {
            self.runs.clone_from(&other.runs);
            return true;
        }
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.runs.len() + other.runs.len());
        let (mut i, mut j) = (0, 0);
        while i < self.runs.len() || j < other.runs.len() {
            let next = match (self.runs.get(i), other.runs.get(j)) {
                (Some(&a), Some(&b)) if a.0 <= b.0 => {
                    i += 1;
                    a
                }
                (_, Some(&b)) => {
                    j += 1;
                    b
                }
                (Some(&a), None) => {
                    i += 1;
                    a
                }
                (None, None) => unreachable!("one is left"),
            };
            match merged.last_mut() {
                Some(last) if next.0 <= last.1 => last.1 = last.1.max(next.1),
                _ => merged.push(next),
            }
        }
        let grew = merged != self.runs;
        self.runs = merged;
        grew
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_and_contains() {
        let mut a = IntervalSet::new();
        a.push(0, 2);
        a.push(2, 4);
        a.push(6, 8);
        assert_eq!(a.runs(), &[(0, 4), (6, 8)]);
        let mut b = IntervalSet::new();
        b.push(3, 7);
        b.push(10, 11);
        assert!(a.union(&b));
        assert_eq!(a.runs(), &[(0, 8), (10, 11)]);
        assert!(!a.union(&b));
        assert!(a.contains(7) && !a.contains(8) && a.contains(10));
        a.insert(8, 10);
        assert_eq!(a.runs(), &[(0, 11)]);
    }
}
