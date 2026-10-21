//! Types, interned.
//!
//! A [`Ty`] is a 4-byte handle into one process-wide table of [`TyKind`]s,
//! so types compare and hash as integers, and a type that contains types
//! (`*const T`, `(A, B)`) stays `Copy`. The primitive types are interned
//! first, so [`Ty::BOOL`] and [`Ty::int`] are constants. Lists of types (a
//! tuple's elements, a struct's generic arguments) are interned too, and
//! live for the rest of the process, like the table.
//!
//! A generic item's types mention its parameters ([`TyKind::Param`]);
//! [`Ty::subst`] replaces them with the arguments of one use of it.
//!
//! Interning takes a lock, but reading a type's kind does not: kinds
//! live in an append-only table of write-once slots, as the symbol
//! interner's strings do, since type checking reads them constantly.

use std::fmt;
use std::sync::{LazyLock, OnceLock, RwLock};

use rustc_hash::FxHashMap;

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
    /// A struct, with its generic arguments.
    Adt(AdtId, GenericArgs),
    /// A generic parameter of the item whose types these are.
    Param(ParamTy),
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

/// The generic arguments of a use of a generic item: one type per
/// parameter, in order, the `impl` block's first for an associated item.
pub type GenericArgs = &'static [Ty];

/// A generic parameter: the `index`th of its item's, counting the `impl`
/// block's first, as in [`GenericArgs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParamTy {
    pub index: u32,
    pub name: Symbol,
}

/// A type variable of one body's inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InferVar(pub(crate) u32);

impl InferVar {
    /// `_` written in a type in a body, before inference gives it a
    /// variable of its own.
    pub(crate) const PLACEHOLDER: InferVar = InferVar(u32::MAX);
}

/// The interning side: what each kind's and list's index is.
struct Table {
    len: u32,
    ids: FxHashMap<TyKind, u32>,
    lists: FxHashMap<Box<[Ty]>, &'static [Ty]>,
}

/// Each type's kind and flags ([`HAS_PARAM`], [`HAS_INFER`]), by index,
/// written once when it is interned. Bucket `b` holds `FIRST << b`
/// slots, so the table grows without moving what it holds.
struct Kinds {
    buckets: [OnceLock<Box<[Slot]>>; BUCKETS],
}

type Slot = OnceLock<(TyKind, u8)>;

const FIRST: u32 = 64;
const BUCKETS: usize = 26;

impl Kinds {
    /// The bucket and slot of index `i`.
    fn locate(i: u32) -> (usize, usize) {
        let n = i / FIRST + 1;
        let bucket = (31 - n.leading_zeros()) as usize;
        let start = FIRST * ((1 << bucket) - 1);
        (bucket, (i - start) as usize)
    }

    fn get(&self, i: u32) -> Option<(TyKind, u8)> {
        let (bucket, slot) = Kinds::locate(i);
        self.buckets[bucket].get()?[slot].get().copied()
    }

    fn set(&self, i: u32, value: (TyKind, u8)) {
        let (bucket, slot) = Kinds::locate(i);
        let slots = self.buckets[bucket]
            .get_or_init(|| (0..FIRST << bucket).map(|_| OnceLock::new()).collect());
        slots[slot].set(value).expect("each type is written once");
    }
}

static KINDS: Kinds = Kinds {
    buckets: [const { OnceLock::new() }; BUCKETS],
};

/// The type contains a generic parameter.
const HAS_PARAM: u8 = 1;
/// The type contains an inference variable.
const HAS_INFER: u8 = 2;

/// Interned in this order, so the constants below are their indices.
const PREDEFINED: [TyKind; 4] = [
    TyKind::Error,
    TyKind::Tuple(&[]),
    TyKind::Bool,
    TyKind::Never,
];

static TABLE: LazyLock<RwLock<Table>> = LazyLock::new(|| {
    let mut table = Table {
        len: 0,
        ids: FxHashMap::default(),
        lists: FxHashMap::default(),
    };
    let ints = IntTy::ALL.map(TyKind::Int);
    for kind in PREDEFINED.into_iter().chain(ints) {
        table.insert(kind);
    }
    RwLock::new(table)
});

impl Table {
    fn insert(&mut self, kind: TyKind) -> u32 {
        let of = |ty: Ty| KINDS.get(ty.0).map_or(0, |(_, flags)| flags);
        let flags = match kind {
            TyKind::Param(_) => HAS_PARAM,
            TyKind::Infer(_) => HAS_INFER,
            TyKind::Tuple(list) | TyKind::Adt(_, list) => list.iter().fold(0, |f, &t| f | of(t)),
            TyKind::RawPtr(_, t) | TyKind::Ref(_, t) => of(t),
            TyKind::Int(_) | TyKind::Bool | TyKind::Never | TyKind::CStr | TyKind::Error => 0,
        };
        let id = self.len;
        self.len += 1;
        // Written before the index is published, so whoever has the
        // index finds it.
        KINDS.set(id, (kind, flags));
        self.ids.insert(kind, id);
        id
    }
}

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
        Ty(table.insert(kind))
    }

    pub fn kind(self) -> TyKind {
        self.entry().0
    }

    fn entry(self) -> (TyKind, u8) {
        match KINDS.get(self.0) {
            Some(entry) => entry,
            // The predefined types exist before anything is interned.
            None => {
                LazyLock::force(&TABLE);
                KINDS.get(self.0).expect("types are interned before use")
            }
        }
    }

    pub fn infer(var: InferVar) -> Ty {
        Ty::new(TyKind::Infer(var))
    }

    pub fn raw_ptr(mutability: Mutability, pointee: Ty) -> Ty {
        Ty::new(TyKind::RawPtr(mutability, pointee))
    }

    /// `types`, interned.
    pub fn list(types: &[Ty]) -> &'static [Ty] {
        if types.is_empty() {
            return &[];
        }
        if let Some(&list) = TABLE.read().unwrap().lists.get(types) {
            return list;
        }
        let mut table = TABLE.write().unwrap();
        match table.lists.get(types) {
            Some(&list) => list,
            None => {
                let list: &'static [Ty] = Box::leak(types.into());
                table.lists.insert(types.into(), list);
                list
            }
        }
    }

    pub fn tuple(elements: &[Ty]) -> Ty {
        Ty::new(TyKind::Tuple(Ty::list(elements)))
    }

    pub fn adt(strukt: StructId, name: Symbol, args: &[Ty]) -> Ty {
        Ty::new(TyKind::Adt(AdtId { strukt, name }, Ty::list(args)))
    }

    pub fn param(index: u32, name: Symbol) -> Ty {
        Ty::new(TyKind::Param(ParamTy { index, name }))
    }

    fn flags(self) -> u8 {
        self.entry().1
    }

    /// Whether the type mentions a generic parameter.
    pub fn has_params(self) -> bool {
        self.flags() & HAS_PARAM != 0
    }

    /// Whether the type contains an inference variable.
    pub fn has_infer(self) -> bool {
        self.flags() & HAS_INFER != 0
    }

    /// The type with each generic parameter replaced by its argument in
    /// `args`.
    pub fn subst(self, args: &[Ty]) -> Ty {
        if !self.has_params() {
            return self;
        }
        match self.kind() {
            TyKind::Param(param) => args.get(param.index as usize).copied().unwrap_or(Ty::ERROR),
            TyKind::Tuple(elements) => Ty::tuple(&subst_list(elements, args)),
            TyKind::Adt(adt, list) => Ty::new(TyKind::Adt(adt, Ty::list(&subst_list(list, args)))),
            TyKind::RawPtr(m, pointee) => Ty::raw_ptr(m, pointee.subst(args)),
            TyKind::Ref(m, pointee) => Ty::new(TyKind::Ref(m, pointee.subst(args))),
            _ => self,
        }
    }

    /// Whether values of this type are copied rather than moved. Structs
    /// are not, until flux has `#[derive(Clone, Copy)]`, and neither are
    /// generic parameters, until they can be bounded by `Copy`.
    pub fn is_copy(self) -> bool {
        match self.kind() {
            TyKind::Tuple(elements) => elements.iter().all(|t| t.is_copy()),
            TyKind::Adt(..) | TyKind::Param(_) => false,
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

/// Each of `list` substituted with `args`.
pub fn subst_list(list: &[Ty], args: &[Ty]) -> Vec<Ty> {
    list.iter().map(|t| t.subst(args)).collect()
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
            TyKind::Adt(adt, []) => write!(f, "{}", adt.name),
            TyKind::Adt(adt, args) => {
                write!(f, "{}<", adt.name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                f.write_str(">")
            }
            TyKind::Param(param) => write!(f, "{}", param.name),
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
