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

use flux_hir::{FunctionId, StructId, TraitId};
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
    /// `<T as Tr>::Out`: a trait's associated type for a type, where it
    /// cannot be replaced by what it is (yet).
    Alias(AliasTy),
    /// `!`
    Never,
    /// `*const T` and `*mut T`.
    RawPtr(Mutability, Ty),
    /// `&T` and `&mut T`.
    Ref(Mutability, Ty),
    /// `[T; N]`, with its length evaluated.
    Array(Ty, u64),
    /// `[T]`: unsized, so only behind a pointer.
    Slice(Ty),
    /// `core::ffi::CStr`: a NUL-terminated string, only behind a reference.
    CStr,
    /// The type of a function item, `fn(i32) -> i32 {double}`: one per
    /// function and generic arguments, with no value to hold, which
    /// coerces to a function pointer of its signature.
    FnDef(FnDefTy),
    /// A function pointer, `fn(i32) -> i32`.
    FnPtr(FnSigTy),
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

/// `<T as Tr<A>>::Out`: associated type `name` of trait `trait_`, with
/// `args` its `Self` then the trait's own arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AliasTy {
    pub trait_: TraitId,
    pub trait_name: Symbol,
    pub name: Symbol,
    pub args: GenericArgs,
}

impl AliasTy {
    pub fn self_ty(self) -> Ty {
        self.args[0]
    }
}

/// A function's signature as a type: its parameters' types then its
/// return type, and whether calling it is `unsafe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FnSigTy {
    pub inputs_output: &'static [Ty],
    pub is_unsafe: bool,
}

impl FnSigTy {
    pub fn new(params: &[Ty], ret: Ty, is_unsafe: bool) -> FnSigTy {
        let mut all = params.to_vec();
        all.push(ret);
        FnSigTy {
            inputs_output: Ty::list(&all),
            is_unsafe,
        }
    }

    pub fn params(self) -> &'static [Ty] {
        &self.inputs_output[..self.inputs_output.len() - 1]
    }

    pub fn ret(self) -> Ty {
        self.inputs_output[self.inputs_output.len() - 1]
    }

    /// The signature with each type replaced by `f`'s.
    pub fn map(self, f: impl FnMut(Ty) -> Ty) -> FnSigTy {
        let list: Vec<Ty> = self.inputs_output.iter().copied().map(f).collect();
        FnSigTy {
            inputs_output: Ty::list(&list),
            ..self
        }
    }
}

/// A function item's type: the function, its name for messages, its
/// generic arguments, and its signature with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FnDefTy {
    pub function: FunctionId,
    pub name: Symbol,
    pub args: GenericArgs,
    pub sig: FnSigTy,
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
/// The type contains an associated type.
const HAS_ALIAS: u8 = 4;

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
            TyKind::Alias(alias) => alias.args.iter().fold(HAS_ALIAS, |f, &t| f | of(t)),
            TyKind::RawPtr(_, t) | TyKind::Ref(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
                of(t)
            }
            TyKind::FnPtr(sig) => sig.inputs_output.iter().fold(0, |f, &t| f | of(t)),
            TyKind::FnDef(def) => def
                .args
                .iter()
                .chain(def.sig.inputs_output)
                .fold(0, |f, &t| f | of(t)),
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

    /// Whether the type contains an associated type.
    pub fn has_aliases(self) -> bool {
        self.flags() & HAS_ALIAS != 0
    }

    pub fn alias(alias: AliasTy) -> Ty {
        Ty::new(TyKind::Alias(alias))
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
            TyKind::Alias(alias) => Ty::alias(AliasTy {
                args: Ty::list(&subst_list(alias.args, args)),
                ..alias
            }),
            TyKind::RawPtr(m, pointee) => Ty::raw_ptr(m, pointee.subst(args)),
            TyKind::Ref(m, pointee) => Ty::new(TyKind::Ref(m, pointee.subst(args))),
            TyKind::Array(elem, len) => Ty::array(elem.subst(args), len),
            TyKind::Slice(elem) => Ty::slice(elem.subst(args)),
            TyKind::FnDef(_) | TyKind::FnPtr(_) => self.map_fn(|t| t.subst(args)),
            _ => self,
        }
    }

    pub fn array(elem: Ty, len: u64) -> Ty {
        Ty::new(TyKind::Array(elem, len))
    }

    pub fn slice(elem: Ty) -> Ty {
        Ty::new(TyKind::Slice(elem))
    }

    /// The element type of an array or slice.
    pub fn sequence_elem(self) -> Option<Ty> {
        match self.kind() {
            TyKind::Array(elem, _) | TyKind::Slice(elem) => Some(elem),
            _ => None,
        }
    }

    /// For a function item or pointer type, the type with each type in
    /// it (generic arguments and signature) replaced by `f`'s; any other
    /// type unchanged.
    pub fn map_fn(self, mut f: impl FnMut(Ty) -> Ty) -> Ty {
        match self.kind() {
            TyKind::FnPtr(sig) => Ty::new(TyKind::FnPtr(sig.map(f))),
            TyKind::FnDef(def) => {
                let args: Vec<Ty> = def.args.iter().map(|&a| f(a)).collect();
                Ty::new(TyKind::FnDef(FnDefTy {
                    args: Ty::list(&args),
                    sig: def.sig.map(f),
                    ..def
                }))
            }
            _ => self,
        }
    }

    /// The types directly inside this one.
    pub fn children(self) -> Vec<Ty> {
        match self.kind() {
            TyKind::Tuple(list) | TyKind::Adt(_, list) => list.to_vec(),
            TyKind::Alias(alias) => alias.args.to_vec(),
            TyKind::RawPtr(_, t) | TyKind::Ref(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
                vec![t]
            }
            TyKind::FnPtr(sig) => sig.inputs_output.to_vec(),
            TyKind::FnDef(def) => def
                .args
                .iter()
                .chain(def.sig.inputs_output)
                .copied()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The signature of a function item or pointer type.
    pub fn fn_sig(self) -> Option<FnSigTy> {
        match self.kind() {
            TyKind::FnPtr(sig) => Some(sig),
            TyKind::FnDef(def) => Some(def.sig),
            _ => None,
        }
    }

    /// Whether the type is a slice, which has no size of its own.
    pub fn is_unsized(self) -> bool {
        matches!(self.kind(), TyKind::Slice(_))
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
            TyKind::Alias(alias) => {
                write!(f, "<{} as {}", alias.args[0], alias.trait_name)?;
                if alias.args.len() > 1 {
                    let args: Vec<String> = alias.args[1..].iter().map(Ty::to_string).collect();
                    write!(f, "<{}>", args.join(", "))?;
                }
                write!(f, ">::{}", alias.name)
            }
            TyKind::Never => f.write_str("!"),
            TyKind::RawPtr(Mutability::Not, pointee) => write!(f, "*const {pointee}"),
            TyKind::RawPtr(Mutability::Mut, pointee) => write!(f, "*mut {pointee}"),
            TyKind::Ref(Mutability::Not, pointee) => write!(f, "&{pointee}"),
            TyKind::Ref(Mutability::Mut, pointee) => write!(f, "&mut {pointee}"),
            TyKind::Array(elem, len) => write!(f, "[{elem}; {len}]"),
            TyKind::Slice(elem) => write!(f, "[{elem}]"),
            TyKind::CStr => f.write_str("CStr"),
            TyKind::FnPtr(sig) => write_sig(f, sig),
            TyKind::FnDef(def) => {
                write_sig(f, def.sig)?;
                write!(f, " {{{}", def.name)?;
                if !def.args.is_empty() {
                    let args: Vec<String> = def.args.iter().map(Ty::to_string).collect();
                    write!(f, "::<{}>", args.join(", "))?;
                }
                f.write_str("}")
            }
            TyKind::Infer(_) => f.write_str("_"),
            TyKind::Error => f.write_str("{error}"),
        }
    }
}

/// `fn(A, B) -> R`, with `unsafe` before it if it is, and without the
/// return type if it is `()`.
fn write_sig(f: &mut fmt::Formatter<'_>, sig: FnSigTy) -> fmt::Result {
    if sig.is_unsafe {
        f.write_str("unsafe ")?;
    }
    let params: Vec<String> = sig.params().iter().map(Ty::to_string).collect();
    write!(f, "fn({})", params.join(", "))?;
    if sig.ret() != Ty::UNIT {
        write!(f, " -> {}", sig.ret())?;
    }
    Ok(())
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
