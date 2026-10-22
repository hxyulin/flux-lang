//! Indexing: `a[i]` on types other than arrays and slices indexed by a
//! `usize`, which are built in, is `*Index::index(&a, i)`, or
//! `*IndexMut::index_mut(&mut a, i)` where it is assigned to or borrowed
//! mutably.

/// What `container[index]` is, when it is read.
#[lang = "index"]
#[diagnostic::on_unimplemented(message = "the type `{Self}` cannot be indexed by `{Idx}`")]
pub trait Index<Idx> {
    /// What indexing gives: `[T]` for a slice indexed by a range (`?Sized`
    /// in rustc, which flux does not check).
    type Output;

    fn index(&self, index: Idx) -> &Self::Output;
}

/// What `container[index]` is, when it is assigned to or borrowed
/// mutably.
#[lang = "index_mut"]
#[diagnostic::on_unimplemented(message = "the type `{Self}` cannot be mutably indexed by `{Idx}`")]
pub trait IndexMut<Idx>: Index<Idx> {
    fn index_mut(&mut self, index: Idx) -> &mut Self::Output;
}
