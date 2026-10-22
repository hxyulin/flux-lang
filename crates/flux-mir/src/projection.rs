//! Immutable place projections, allocated together and shared on clone.

use std::cell::RefCell;
use std::ops::Deref;

use flux_arena::{SharedSlice, SharedSlicePool};

use crate::ProjectionElem;

thread_local! {
    // The pool holds only a weak reference to its current chunk. It does
    // not keep a compilation's projections alive after its MIR is dropped.
    static POOL: RefCell<SharedSlicePool<ProjectionElem>> = RefCell::default();
}

/// A place's projection chain. Clones and prefixes share their storage;
/// new chains are copied directly into reference-counted chunks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Projection(SharedSlice<ProjectionElem>);

impl Projection {
    pub fn from_exact_iter<I>(elements: I) -> Self
    where
        I: IntoIterator<Item = ProjectionElem>,
        I::IntoIter: ExactSizeIterator,
    {
        POOL.with(|pool| {
            if let Ok(mut pool) = pool.try_borrow_mut() {
                Self(pool.alloc(elements))
            } else {
                // Iterator callbacks may themselves construct projections.
                // Their slices own their chunks, so a temporary pool is enough.
                Self(SharedSlicePool::default().alloc(elements))
            }
        })
    }

    pub fn prefix(&self, len: usize) -> Self {
        Self(self.0.slice(0..len))
    }

    pub fn appended(&self, element: ProjectionElem) -> Self {
        Self::from_exact_iter((0..self.len() + 1).map(|i| self.get(i).copied().unwrap_or(element)))
    }
}

impl Deref for Projection {
    type Target = [ProjectionElem];
    fn deref(&self) -> &[ProjectionElem] {
        &self.0
    }
}

impl AsRef<[ProjectionElem]> for Projection {
    fn as_ref(&self) -> &[ProjectionElem] {
        self
    }
}

impl From<&[ProjectionElem]> for Projection {
    fn from(elements: &[ProjectionElem]) -> Self {
        Self::from_exact_iter(elements.iter().copied())
    }
}

impl From<Vec<ProjectionElem>> for Projection {
    fn from(elements: Vec<ProjectionElem>) -> Self {
        Self::from_exact_iter(elements)
    }
}

impl From<Box<[ProjectionElem]>> for Projection {
    fn from(elements: Box<[ProjectionElem]>) -> Self {
        Self::from(elements.as_ref())
    }
}

impl<const N: usize> From<[ProjectionElem; N]> for Projection {
    fn from(elements: [ProjectionElem; N]) -> Self {
        Self::from_exact_iter(elements)
    }
}

impl FromIterator<ProjectionElem> for Projection {
    fn from_iter<I: IntoIterator<Item = ProjectionElem>>(elements: I) -> Self {
        Self::from(elements.into_iter().collect::<Vec<_>>())
    }
}

impl<'a> IntoIterator for &'a Projection {
    type Item = &'a ProjectionElem;
    type IntoIter = std::slice::Iter<'a, ProjectionElem>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_and_prefixes_survive_further_projections() {
        let projection = Projection::from([ProjectionElem::Deref, ProjectionElem::Field(3)]);
        let clone = projection.clone();
        let prefix = projection.prefix(1);
        let longer = projection.appended(ProjectionElem::Field(4));
        assert_eq!(projection, clone);
        assert_eq!(projection.as_ptr(), clone.as_ptr());
        assert_eq!(projection.as_ptr(), prefix.as_ptr());
        assert_eq!(&*prefix, &[ProjectionElem::Deref]);
        drop((projection, clone));
        assert_eq!(
            &longer[..2],
            &[ProjectionElem::Deref, ProjectionElem::Field(3)]
        );
    }

    #[test]
    fn iterator_callbacks_can_construct_projections() {
        let projection = Projection::from_exact_iter((0..3).map(|i| {
            let nested = Projection::from([ProjectionElem::Field(i)]);
            nested[0]
        }));
        assert_eq!(
            &*projection,
            &[
                ProjectionElem::Field(0),
                ProjectionElem::Field(1),
                ProjectionElem::Field(2),
            ]
        );
    }
}
