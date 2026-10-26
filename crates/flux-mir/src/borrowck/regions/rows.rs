//! Checked views of a normalized type's region slots.
use super::Var;
use flux_hir::ItemTree;
use flux_ty::Ty;

#[derive(Clone, Copy)]
pub(super) struct RegionRow<'a> {
    pub ty: Ty,
    pub vars: &'a [Var],
}

impl<'a> RegionRow<'a> {
    /// The caller normalizes first. No row can be sliced using an erased
    /// projection's slot count and normalized afterwards.
    pub fn new(items: &ItemTree, normalized: Ty, vars: &'a [Var]) -> Option<Self> {
        (flux_ty::regions::slots(items, normalized) == vars.len()).then_some(Self {
            ty: normalized,
            vars,
        })
    }

    pub fn cursor(self) -> Cursor<'a> {
        Cursor { rest: self.vars }
    }
}

pub(super) struct Cursor<'a> {
    rest: &'a [Var],
}
impl<'a> Cursor<'a> {
    pub fn roots(&mut self, count: usize) -> Option<&'a [Var]> {
        if count > self.rest.len() {
            return None;
        }
        let (head, tail) = self.rest.split_at(count);
        self.rest = tail;
        Some(head)
    }
    pub fn child(&mut self, items: &ItemTree, normalized: Ty) -> Option<RegionRow<'a>> {
        let vars = self.roots(flux_ty::regions::slots(items, normalized))?;
        RegionRow::new(items, normalized, vars)
    }
    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rows_preserve_child_order_and_reject_misalignment() {
        let (_, _, hir) = flux_hir::lower_text("rows.rs", "fn main() {}");
        let reference = Ty::new(flux_ty::TyKind::Ref(
            flux_ty::Mutability::Not,
            Ty::int(flux_ty::IntTy::U32),
        ));
        let pair = Ty::tuple(&[reference, reference]);
        assert!(RegionRow::new(&hir.items, pair, &[7]).is_none());
        let row = RegionRow::new(&hir.items, pair, &[7, 11]).unwrap();
        let mut cursor = row.cursor();
        assert_eq!(cursor.child(&hir.items, reference).unwrap().vars, &[7]);
        assert_eq!(cursor.child(&hir.items, reference).unwrap().vars, &[11]);
        assert!(cursor.is_empty());
        assert!(cursor.child(&hir.items, reference).is_none());
    }
}
