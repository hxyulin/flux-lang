//! Types, interned.
//!
//! A [`Ty`] is a 4-byte handle into one process-wide table of [`TyKind`]s,
//! so types compare and hash as integers, and a type that contains types
//! (`*const T`, `(A, B)`) stays `Copy`. The primitive types are interned
//! first, so [`Ty::BOOL`] and [`Ty::int`] are constants. A tuple's element
//! list is interned too, and lives for the rest of the process, like the
//! table.

use std::collections::HashMap;
use std::fmt;
use std::sync::{LazyLock, RwLock};

use flux_hir::StructId;
use flux_intern::{Symbol, sym};

/// A type: a handle to its [`TyKind`].
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ty(u32);

/// What a type is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TyKind {
    Int(IntTy),
    Bool,
    /// `(A, B)`; `()` is the empty tuple, [`Ty::UNIT`].
    Tuple(&'static [Ty]),
    /// A struct.
    Adt(AdtId),
    /// `!`
    Never,
    /// `*const T` and `*mut T`.
    RawPtr(Mutability, Ty),
    /// `&T` and `&mut T`. Only `&CStr` exists so far, from `c"..."`.
    Ref(Mutability, Ty),
    /// `core::ffi::CStr`: a NUL-terminated string, only behind a reference.
    CStr,
    /// An inference variable; never present after inference.
    Infer(InferVar),
    /// The type of an erroneous expression, already reported. It unifies
    /// with everything, so one error does not cause others.
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mutability {
    Not,
    Mut,
}

/// An integer type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
}

/// A struct, and its name for messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AdtId {
    pub strukt: StructId,
    pub name: Symbol,
}

/// A type variable of one body's inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InferVar(pub(crate) u32);

struct Table {
    kinds: Vec<TyKind>,
    ids: HashMap<TyKind, u32>,
    lists: HashMap<Box<[Ty]>, &'static [Ty]>,
}

/// Interned in this order, so the constants below are their indices.
const PREDEFINED: [TyKind; 4] = [
    TyKind::Error,
    TyKind::Tuple(&[]),
    TyKind::Bool,
    TyKind::Never,
];

static TABLE: LazyLock<RwLock<Table>> = LazyLock::new(|| {
    let mut table = Table {
        kinds: Vec::new(),
        ids: HashMap::new(),
        lists: HashMap::new(),
    };
    let ints = IntTy::ALL.map(TyKind::Int);
    for kind in PREDEFINED.into_iter().chain(ints) {
        table.ids.insert(kind, table.kinds.len() as u32);
        table.kinds.push(kind);
    }
    RwLock::new(table)
});

impl Ty {
    pub const ERROR: Ty = Ty(0);
    pub const UNIT: Ty = Ty(1);
    pub const BOOL: Ty = Ty(2);
    pub const NEVER: Ty = Ty(3);

    pub const fn int(int: IntTy) -> Ty {
        Ty(PREDEFINED.len() as u32 + int as u32)
    }

    /// The type with this kind, interning it if it is new.
    pub fn new(kind: TyKind) -> Ty {
        if let Some(&id) = TABLE.read().unwrap().ids.get(&kind) {
            return Ty(id);
        }
        let mut table = TABLE.write().unwrap();
        if let Some(&id) = table.ids.get(&kind) {
            return Ty(id);
        }
        let id = table.kinds.len() as u32;
        table.kinds.push(kind);
        table.ids.insert(kind, id);
        Ty(id)
    }

    pub fn kind(self) -> TyKind {
        TABLE.read().unwrap().kinds[self.0 as usize]
    }

    pub fn infer(var: InferVar) -> Ty {
        Ty::new(TyKind::Infer(var))
    }

    pub fn raw_ptr(mutability: Mutability, pointee: Ty) -> Ty {
        Ty::new(TyKind::RawPtr(mutability, pointee))
    }

    pub fn tuple(elements: &[Ty]) -> Ty {
        let interned = TABLE.read().unwrap().lists.get(elements).copied();
        let elements = match interned {
            Some(list) => list,
            None => {
                let mut table = TABLE.write().unwrap();
                match table.lists.get(elements) {
                    Some(&list) => list,
                    None => {
                        let list: &'static [Ty] = Box::leak(elements.into());
                        table.lists.insert(elements.into(), list);
                        list
                    }
                }
            }
        };
        Ty::new(TyKind::Tuple(elements))
    }

    pub fn adt(strukt: StructId, name: Symbol) -> Ty {
        Ty::new(TyKind::Adt(AdtId { strukt, name }))
    }

    /// Whether values of this type are copied rather than moved. Structs
    /// are not, until flux has `#[derive(Clone, Copy)]`.
    pub fn is_copy(self) -> bool {
        match self.kind() {
            TyKind::Tuple(elements) => elements.iter().all(|t| t.is_copy()),
            TyKind::Adt(_) => false,
            TyKind::Ref(Mutability::Mut, _) => false,
            _ => true,
        }
    }

    /// The integer type, if this is one.
    pub fn as_int(self) -> Option<IntTy> {
        match self.kind() {
            TyKind::Int(int) => Some(int),
            _ => None,
        }
    }

    pub fn is_signed_int(self) -> bool {
        self.as_int().is_some_and(IntTy::is_signed)
    }

    /// `*const T`, `*mut T`, `&T` or `&mut T`.
    pub fn is_pointer(self) -> bool {
        matches!(self.kind(), TyKind::RawPtr(..) | TyKind::Ref(..))
    }
}

impl fmt::Debug for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ty({self})")
    }
}

impl IntTy {
    pub const ALL: [IntTy; 12] = [
        IntTy::I8,
        IntTy::I16,
        IntTy::I32,
        IntTy::I64,
        IntTy::I128,
        IntTy::Isize,
        IntTy::U8,
        IntTy::U16,
        IntTy::U32,
        IntTy::U64,
        IntTy::U128,
        IntTy::Usize,
    ];

    pub fn is_signed(self) -> bool {
        matches!(
            self,
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::I128 | IntTy::Isize
        )
    }

    /// Width in bits. Both targets flux supports are 64-bit, so `isize`
    /// and `usize` are 64 bits wide.
    pub fn bits(self) -> u32 {
        match self {
            IntTy::I8 | IntTy::U8 => 8,
            IntTy::I16 | IntTy::U16 => 16,
            IntTy::I32 | IntTy::U32 => 32,
            IntTy::I64 | IntTy::U64 | IntTy::Isize | IntTy::Usize => 64,
            IntTy::I128 | IntTy::U128 => 128,
        }
    }

    /// The largest value, as an unsigned magnitude.
    pub fn max(self) -> u128 {
        let bits = self.bits() - u32::from(self.is_signed());
        if bits == 128 {
            u128::MAX
        } else {
            (1u128 << bits) - 1
        }
    }

    pub fn name(self) -> Symbol {
        match self {
            IntTy::I8 => sym::i8,
            IntTy::I16 => sym::i16,
            IntTy::I32 => sym::i32,
            IntTy::I64 => sym::i64,
            IntTy::I128 => sym::i128,
            IntTy::Isize => sym::isize,
            IntTy::U8 => sym::u8,
            IntTy::U16 => sym::u16,
            IntTy::U32 => sym::u32,
            IntTy::U64 => sym::u64,
            IntTy::U128 => sym::u128,
            IntTy::Usize => sym::usize,
        }
    }

    pub fn from_name(name: Symbol) -> Option<IntTy> {
        IntTy::ALL.into_iter().find(|ty| ty.name() == name)
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            TyKind::Int(int) => write!(f, "{}", int.name()),
            TyKind::Bool => f.write_str("bool"),
            TyKind::Tuple([one]) => write!(f, "({one},)"),
            TyKind::Tuple(elements) => {
                f.write_str("(")?;
                for (i, element) in elements.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{element}")?;
                }
                f.write_str(")")
            }
            TyKind::Adt(adt) => write!(f, "{}", adt.name),
            TyKind::Never => f.write_str("!"),
            TyKind::RawPtr(Mutability::Not, pointee) => write!(f, "*const {pointee}"),
            TyKind::RawPtr(Mutability::Mut, pointee) => write!(f, "*mut {pointee}"),
            TyKind::Ref(Mutability::Not, pointee) => write!(f, "&{pointee}"),
            TyKind::Ref(Mutability::Mut, pointee) => write!(f, "&mut {pointee}"),
            TyKind::CStr => f.write_str("CStr"),
            TyKind::Infer(_) => f.write_str("_"),
            TyKind::Error => f.write_str("{error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning() {
        assert_eq!(Ty::new(TyKind::Bool), Ty::BOOL);
        assert_eq!(Ty::new(TyKind::Int(IntTy::U16)), Ty::int(IntTy::U16));
        assert_eq!(Ty::int(IntTy::Usize).kind(), TyKind::Int(IntTy::Usize));
        let p = Ty::raw_ptr(Mutability::Not, Ty::int(IntTy::I8));
        assert_eq!(p, Ty::raw_ptr(Mutability::Not, Ty::int(IntTy::I8)));
        assert_ne!(p, Ty::raw_ptr(Mutability::Mut, Ty::int(IntTy::I8)));
        assert_eq!(p.to_string(), "*const i8");
        assert_eq!(Ty::tuple(&[]), Ty::UNIT);
        let pair = Ty::tuple(&[Ty::BOOL, p]);
        assert_eq!(pair, Ty::tuple(&[Ty::BOOL, p]));
        assert_eq!(pair.to_string(), "(bool, *const i8)");
        assert_eq!(Ty::tuple(&[Ty::BOOL]).to_string(), "(bool,)");
    }
}
